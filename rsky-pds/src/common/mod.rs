use anyhow::{Result};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;

pub fn struct_to_cbor<T: Serialize> (
    obj: T
) -> Result<Vec<u8>> {
    // Encode object to json before dag-cbor because serde_ipld_dagcbor doesn't properly
    // sort by keys
    let json = serde_json::to_string(&obj)?;
    // Deserialize to IndexMap with preserve key order enabled. serde_ipld_dagcbor does not sort nested
    // objects properly by keys
    let map: IndexMap<String, Value> = serde_json::from_str(&json)?;
    let cbor_bytes = serde_ipld_dagcbor::to_vec(&map)?;

    Ok(cbor_bytes)
}

pub mod ipld;