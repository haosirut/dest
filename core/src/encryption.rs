//! Legacy encryption module — prefer `crypto` module for new code.
//! This module is kept for backward compatibility.

pub use crate::crypto::{
    encrypt_data, decrypt_data, encrypt_file, decrypt_to_file,
    derive_key_from_password, generate_salt,
};
