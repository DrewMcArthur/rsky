use anyhow::Result;
use libipld::cid::Cid;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TypedJsonBlobRef {
    #[serde(rename = "$type")]
    pub _type: String,
    pub ref_: Cid,
    pub mime_type: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UntypedJsonBlobRef {
    pub cid: String,
    pub mime_type: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum JsonBlobRef {
    Typed(TypedJsonBlobRef),
    Untyped(UntypedJsonBlobRef),
}

#[derive(Clone, Debug)]
pub struct BlobRef {
    pub ref_: Cid,
    pub mime_type: String,
    pub size: i64,
    pub original: JsonBlobRef,
}

impl BlobRef {
    pub fn new(ref_: Cid, mime_type: String, size: i64, original: Option<JsonBlobRef>) -> Self {
        let original = original.unwrap_or(JsonBlobRef::Typed(TypedJsonBlobRef {
            _type: "blob".to_string(),
            ref_: ref_.clone(),
            mime_type: mime_type.clone(),
            size: size as u64,
        }));
        BlobRef {
            ref_,
            mime_type,
            size,
            original,
        }
    }

    pub fn as_blob_ref(obj: &JsonValue) -> Result<BlobRef> {
        Ok(BlobRef::from_json_ref(serde_json::from_value(obj.clone())?))
    }

    pub fn from_json_ref(json: JsonBlobRef) -> Self {
        match json {
            JsonBlobRef::Typed(typed) => BlobRef::new(
                typed.ref_.clone(),
                typed.mime_type.clone(),
                typed.size.clone() as i64,
                Some(JsonBlobRef::Typed(typed)),
            ),
            JsonBlobRef::Untyped(untyped) => BlobRef::new(
                Cid::try_from(untyped.cid.as_str()).unwrap(),
                untyped.mime_type.clone(),
                -1,
                Some(JsonBlobRef::Untyped(untyped)),
            ),
        }
    }

    pub fn ipld(&self) -> TypedJsonBlobRef {
        TypedJsonBlobRef {
            _type: "blob".to_string(),
            ref_: self.ref_.clone(),
            mime_type: self.mime_type.clone(),
            size: self.size as u64,
        }
    }

    pub fn to_json(&self) -> JsonValue {
        serde_json::to_value(self.ipld()).unwrap()
    }

    pub fn cid(&self) -> Result<Cid> {
        match self.original {
            JsonBlobRef::Typed(ref typed) => Ok(typed.ref_.clone()),
            JsonBlobRef::Untyped(ref untyped) => Ok(Cid::try_from(untyped.cid.as_str())?),
        }
    }
}
