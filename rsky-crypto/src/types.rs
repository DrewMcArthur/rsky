use anyhow::Result;

pub struct DidKeyPlugin<'p> {
    pub prefix: [u8; 2],
    pub jwt_alg: &'p str,
    pub compress_pubkey: fn(Vec<u8>) -> Result<Vec<u8>>,
    pub decompress_pubkey: fn(Vec<u8>) -> Result<Vec<u8>>,
    pub verify_signature: fn(&String, &[u8], &[u8], Option<VerifyOptions>) -> Result<bool>,
}

pub struct VerifyOptions {
    pub allow_malleable_sig: Option<bool>,
}

pub trait Didable {
    fn did(&self) -> Result<String>;
}

pub trait Signer {
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64]>;
}

pub trait Keypair: Didable + Signer {}

pub trait ExportableKeypair: Keypair {
    fn export(&self) -> Result<[u8; 32]>;
}
