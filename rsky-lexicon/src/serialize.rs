use crate::blob_refs::BlobRef;
use anyhow::Result;
use rsky_common_web::{
    ipld::{ipld_to_json, json_to_ipld},
    IpldValue,
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum LexValue {
    IpldValue(IpldValue),
    BlobRef(BlobRef),
    Array(Vec<LexValue>),
    Object(HashMap<String, LexValue>),
}

pub type RepoRecord = HashMap<String, LexValue>;

pub fn lex_to_ipld(val: LexValue) -> Result<IpldValue> {
    match val {
        LexValue::Array(arr) => Ok(IpldValue::Array(
            arr.into_iter()
                .map(lex_to_ipld)
                .collect::<Result<Vec<IpldValue>>>()?,
        )),
        LexValue::Object(obj) => {
            let mut to_return = HashMap::new();
            for (key, value) in obj {
                to_return.insert(key, lex_to_ipld(value)?);
            }
            Ok(IpldValue::Object(to_return))
        }
        LexValue::BlobRef(blob_ref) => Ok(IpldValue::Cid(blob_ref.cid()?)),
        LexValue::IpldValue(ipld) => Ok(ipld),
    }
}

pub fn ipld_to_lex(val: IpldValue) -> LexValue {
    match val {
        IpldValue::Array(arr) => LexValue::Array(arr.into_iter().map(ipld_to_lex).collect()),
        IpldValue::Object(obj) => {
            let mut to_return = HashMap::new();
            for (key, value) in obj {
                to_return.insert(key, ipld_to_lex(value));
            }
            LexValue::Object(to_return)
        }
        IpldValue::Bytes(bytes) => LexValue::IpldValue(IpldValue::Bytes(bytes)),
        IpldValue::Cid(cid) => LexValue::IpldValue(IpldValue::Cid(cid)),
        IpldValue::Json(j) => LexValue::IpldValue(IpldValue::Json(j)),
    }
}

pub fn lex_to_json(val: LexValue) -> Result<JsonValue> {
    Ok(ipld_to_json(lex_to_ipld(val)?))
}

pub fn stringify_lex(val: LexValue) -> Result<String> {
    Ok(serde_json::to_string(&lex_to_json(val)?)?)
}

pub fn json_to_lex(val: JsonValue) -> Result<LexValue> {
    Ok(ipld_to_lex(json_to_ipld(val)?))
}

pub fn json_string_to_lex(val: &str) -> Result<LexValue> {
    Ok(json_to_lex(serde_json::from_str(val)?)?)
}
