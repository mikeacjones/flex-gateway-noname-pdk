use serde::Deserialize;
#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(alias = "nn_host", deserialize_with = "pdk::serde::deserialize_service")]
    pub nn_host: pdk::hl::Service,
    #[serde(alias = "nn_index")]
    pub nn_index: i64,
    #[serde(alias = "nn_key")]
    pub nn_key: String,
    #[serde(alias = "nn_type")]
    pub nn_type: i64,
}
#[pdk::hl::entrypoint_flex]
fn init(abi: &dyn pdk::flex_abi::api::FlexAbi) -> Result<(), anyhow::Error> {
    let config: Config = serde_json::from_slice(abi.get_configuration())
        .map_err(|err| {
            anyhow::anyhow!(
                "Failed to parse configuration '{}'. Cause: {}",
                String::from_utf8_lossy(abi.get_configuration()), err
            )
        })?;
    abi.service_create(config.nn_host)?;
    Ok(())
}
