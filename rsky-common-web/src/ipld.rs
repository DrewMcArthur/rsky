use anyhow::Result;
use base64::{self, Engine};
use bytes::Bytes;
use libipld::cid::Cid;
use serde_json::json;
pub use serde_json::Value as JsonValue;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum IpldValue {
    Json(JsonValue),
    Cid(Cid),
    Bytes(Bytes),
    Array(Vec<IpldValue>),
    Object(HashMap<String, IpldValue>),
}

pub fn json_to_ipld(val: JsonValue) -> Result<IpldValue> {
    let decoder = base64::engine::general_purpose::STANDARD;
    match val {
        JsonValue::Array(arr) => Ok(IpldValue::Array(
            arr.into_iter()
                .map(json_to_ipld)
                .collect::<Result<Vec<IpldValue>>>()?,
        )),
        JsonValue::Object(obj) => {
            if let Some(link) = obj.get("$link") {
                if obj.len() == 1 {
                    match link.as_str() {
                        Some(link) => return Ok(IpldValue::Cid(Cid::try_from(link)?)),
                        None => return Err(anyhow::Error::msg("Invalid CID link")),
                    }
                }
            }
            if let Some(bytes) = obj.get("$bytes") {
                if obj.len() == 1 {
                    return Ok(IpldValue::Bytes(Bytes::from(match bytes.as_str() {
                        Some(bytestr) => decoder.decode(bytestr)?,
                        None => Vec::new(),
                    })));
                }
            }
            let mut to_return = HashMap::new();
            for (key, value) in obj {
                to_return.insert(key, json_to_ipld(value)?);
            }
            Ok(IpldValue::Object(to_return))
        }
        _ => Ok(IpldValue::Json(val)),
    }
}

pub fn ipld_to_json(val: IpldValue) -> JsonValue {
    let encoder = base64::engine::general_purpose::STANDARD;
    match val {
        IpldValue::Array(arr) => JsonValue::Array(arr.into_iter().map(ipld_to_json).collect()),
        IpldValue::Object(obj) => {
            let mut to_return = serde_json::Map::new();
            for (key, value) in obj {
                to_return.insert(key, ipld_to_json(value));
            }
            JsonValue::Object(to_return)
        }
        IpldValue::Bytes(bytes) => json!({ "$bytes": encoder.encode(bytes) }),
        IpldValue::Cid(cid) => json!({ "$link": cid.to_string() }),
        IpldValue::Json(json) => json,
    }
}

pub fn ipld_equals(a: &IpldValue, b: &IpldValue) -> bool {
    match (a, b) {
        (IpldValue::Array(arr_a), IpldValue::Array(arr_b)) => {
            if arr_a.len() != arr_b.len() {
                return false;
            }
            for i in 0..arr_a.len() {
                if !ipld_equals(&arr_a[i], &arr_b[i]) {
                    return false;
                }
            }
            true
        }
        (IpldValue::Object(obj_a), IpldValue::Object(obj_b)) => {
            if obj_a.len() != obj_b.len() {
                return false;
            }
            for (key, value_a) in obj_a {
                if let Some(value_b) = obj_b.get(key) {
                    if !ipld_equals(value_a, value_b) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        }
        (IpldValue::Bytes(bytes_a), IpldValue::Bytes(bytes_b)) => bytes_a == bytes_b,
        (IpldValue::Cid(cid_a), IpldValue::Cid(cid_b)) => cid_a == cid_b,
        (IpldValue::Json(json_a), IpldValue::Json(json_b)) => json_a == json_b,
        _ => false,
    }
}
