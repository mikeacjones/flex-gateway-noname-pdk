mod generated;

use anyhow::{anyhow, Result};

use crate::generated::config::Config;
use pdk::hl::*;
use pdk::logger;
use serde_json::json;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/* Function to strip out the envoy information that aren't actually headers */
fn header_cleanup(headers: Vec<(String, String)>) -> Vec<(String, String)> {
    headers
        .into_iter()
        .filter(|(header, _)| !header.starts_with(":"))
        .collect()
}

/* Simple function to grab a value out of a vector of tuples, treating the first item in the tuple as the "key" */
fn get_by_key(kvps: &[(String, String)], key: &str) -> String {
    kvps.iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.to_owned())
        .unwrap_or(String::from(""))
}

/* Get current time stamp */
fn current_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "Time went backwards".to_string())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/* Sends the packet pair to Noname */
async fn send_pp(pp: serde_json::Value, config: &Config, client: HttpClient) {
    logger::debug!("<============= SENDING DATA TO NONAME ANALYTICS ENGINE ==============>");
    let result = client
        .request(&config.nn_host)
        .body(pp.to_string().as_bytes())
        .headers(vec![("Content-Type", "application/json")])
        .post()
        .await;
    match result {
        Ok(_engine_response) => {
            logger::debug!("Sent data to Noname engine");
        }
        Err(error) => {
            logger::info!("Error communicating with Noname engine {:?}", error);
        }
    }
}

async fn request_filter(
    request_state: RequestState,
) -> Flow<(Vec<(String, String)>, Vec<u8>, u64)> {
    let request_ts = current_ts();

    let headers_state = request_state.into_headers_state().await;
    let headers_handler = headers_state.handler();
    let headers: Vec<(String, String)> = headers_handler.headers();

    let body_state = headers_state.into_body_state().await;
    let body_handler = body_state.handler();
    let body = body_handler.body();

    Flow::Continue((headers, body, request_ts))
}

async fn response_filter(
    response_state: ResponseState,
    request_data: RequestData<(Vec<(String, String)>, Vec<u8>, u64)>,
    config: &Config,
    client: HttpClient,
) {
    let RequestData::Continue((original_request_headers, request_body, request_ts)) = request_data
    else {
        logger::debug!("Noname: Request data was not passed to response filter.");
        return;
    };

    let method: String = get_by_key(&original_request_headers, ":method");
    let path: String = get_by_key(&original_request_headers, ":path");
    let host: String = get_by_key(&original_request_headers, ":authority");
    let scheme: String = get_by_key(&original_request_headers, ":scheme");

    let request_headers: Vec<(String, String)> = header_cleanup(original_request_headers);

    let headers_state = response_state.into_headers_state().await;
    let status_code = headers_state.status_code();
    let headers_handler = headers_state.handler();
    let headers = headers_handler.headers();
    let response_headers: Vec<(String, String)> = header_cleanup(headers);

    let body_state = headers_state.into_body_state().await;
    let body_handler = body_state.handler();
    let response_body = body_handler.body();

    let response_ts = current_ts();

    let mut request_headers_map: HashMap<String, String> = request_headers.into_iter().collect();
    request_headers_map.insert(String::from("host"), host);

    let response_headers_map: HashMap<String, String> = response_headers.into_iter().collect();
    let pp: serde_json::Value = json!({
      "source": {
        "type": config.nn_type,
        "index": config.nn_index,
        "key": config.nn_key
      },
      "tcp": {
        "src": 0,
        "dst": 0
      },
      "ip": {
        "v": 4,
        "src": "0.0.0.0",
        "dst": "0.0.0.0"
      },
      "http": {
        "v": "HTTP/1.1",
        "scheme": scheme,
        "request": {
          "ts": request_ts,
          "method": method,
          "url": path,
          "headers": request_headers_map,
          "body": if request_body.len() > 0 { String::from_utf8_lossy(request_body.as_slice()).to_string() } else { "".to_string() }
        },
        "response": {
          "ts": response_ts,
          "status": status_code,
          "headers": response_headers_map,
          "body": if response_body.len() > 0 { String::from_utf8_lossy(response_body.as_slice()).to_string() } else { "".to_string() }
        }
      }
    });

    send_pp(pp, config, client).await
}

#[entrypoint]
async fn configure(launcher: Launcher, Configuration(bytes): Configuration) -> Result<()> {
    let config: Config = serde_json::from_slice(&bytes).map_err(|err| {
        anyhow!(
            "Failed to parse configuration '{}'. Cause: {}",
            String::from_utf8_lossy(&bytes),
            err
        )
    })?;
    let filter = on_request(request_filter)
        .on_response(|r, d, client| response_filter(r, d, &config, client));
    launcher.launch(filter).await?;
    Ok(())
}
