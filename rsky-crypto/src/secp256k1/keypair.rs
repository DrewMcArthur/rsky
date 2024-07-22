use anyhow::Result;
use secp256k1::{hashes::sha256, rand, Message, PublicKey, Secp256k1, SecretKey};

use crate::{
    constants::SECP256K1_JWT_ALG,
    did,
    types::{Didable, ExportableKeypair, Keypair, Signer},
};

#[derive(Default)]
pub struct Secp256k1KeypairOptions {
    pub exportable: Option<bool>,
}

pub struct Secp256k1Keypair {
    jwt_alg: &'static str,
    public_key: PublicKey,
    private_key: SecretKey,
    exportable: bool,
}

impl Signer for Secp256k1Keypair {
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64]> {
        let message = Message::from_hashed_data::<sha256::Hash>(msg);
        let sig = Secp256k1::new().sign_ecdsa(&message, &self.private_key);
        Ok(sig.serialize_compact())
    }
}

impl Keypair for Secp256k1Keypair {}

impl ExportableKeypair for Secp256k1Keypair {
    fn export(&self) -> Result<[u8; 32]> {
        if !self.exportable {
            return Err(anyhow::anyhow!("keypair is not exportable"));
        }
        Ok(self.private_key.secret_bytes())
    }
}

impl Didable for Secp256k1Keypair {
    fn did(&self) -> Result<String> {
        did::format_did_key(
            self.jwt_alg.to_string(),
            self.public_key.serialize().to_vec(),
        )
    }
}

impl Secp256k1Keypair {
    pub fn new(private_key: SecretKey, exportable: bool) -> Result<Self> {
        let k256 = Secp256k1::new();
        Ok(Self {
            jwt_alg: &SECP256K1_JWT_ALG,
            public_key: PublicKey::from_secret_key(&k256, &private_key),
            private_key,
            exportable,
        })
    }

    pub fn create(opts: Option<Secp256k1KeypairOptions>) -> Result<Self> {
        let exportable = opts.unwrap_or_default().exportable.unwrap_or(false);
        let private_key = secp256k1::SecretKey::new(&mut rand::thread_rng());
        Self::new(private_key, exportable)
    }

    pub fn import(private_key: &[u8], opts: Option<Secp256k1KeypairOptions>) -> Result<Self> {
        let exportable = opts.unwrap_or_default().exportable.unwrap_or(false);
        let private_key = SecretKey::from_slice(private_key)?;
        Self::new(private_key, exportable)
    }

    pub fn public_key_bytes(&self) -> [u8; 33] {
        self.public_key.serialize()
    }

    pub fn public_key_str(&self, encoding: Option<String>) -> String {
        unimplemented!()
    }
}
