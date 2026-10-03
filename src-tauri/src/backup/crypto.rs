//! 导出文件的密码学原语：PBKDF2-HMAC-SHA256 派生密钥 + AES-256-GCM 认证加密。
//!
//! 算法参数随文件保存，解密时按文件中的参数重建密钥，因此调整默认参数不会让既有
//! 文件失效。

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use pbkdf2::pbkdf2_hmac_array;
use rand::RngCore;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::{AppError, Result};

/// 派生密钥长度：AES-256 使用 32 字节。
pub const KEY_LEN: usize = 32;
/// PBKDF2 盐长度。
pub const SALT_LEN: usize = 16;
/// AES-GCM 的 nonce 长度（NIST 建议 96 位）。
pub const NONCE_LEN: usize = 12;
/// 新导出文件使用的 PBKDF2 迭代次数。
pub const DEFAULT_ITERATIONS: u32 = 600_000;

/// 随机字节，用于盐与 nonce。
pub fn random_bytes(len: usize) -> Zeroizing<Vec<u8>> {
    let mut bytes = Zeroizing::new(vec![0u8; len]);
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
}

/// 由加密字符串派生 32 字节密钥；返回值离开作用域时清零。
pub fn derive_key(passphrase: &str, salt: &[u8], iterations: u32) -> Zeroizing<[u8; KEY_LEN]> {
    Zeroizing::new(pbkdf2_hmac_array::<Sha256, KEY_LEN>(
        passphrase.as_bytes(),
        salt,
        iterations,
    ))
}

/// AES-256-GCM 加密，返回「密文 + 认证标签」。
pub fn seal(key: &[u8; KEY_LEN], nonce: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    cipher(key)?
        .encrypt(
            Nonce::from_slice(checked(nonce)?),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AppError::internal("加密失败"))
}

/// 解密并校验认证标签；加密字符串错误或内容被篡改都会失败。
pub fn open(key: &[u8; KEY_LEN], nonce: &[u8], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    cipher(key)?
        .decrypt(
            Nonce::from_slice(checked(nonce)?),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AppError::validation("解密失败：加密字符串不正确，或文件已损坏"))
}

fn cipher(key: &[u8; KEY_LEN]) -> Result<Aes256Gcm> {
    Aes256Gcm::new_from_slice(key).map_err(|_| AppError::internal("加密密钥长度无效"))
}

/// `Nonce::from_slice` 要求精确长度，先校验以免损坏的文件触发 panic。
fn checked(nonce: &[u8]) -> Result<&[u8]> {
    if nonce.len() == NONCE_LEN {
        Ok(nonce)
    } else {
        Err(AppError::validation("文件中的 nonce 长度无效"))
    }
}
