//! Provider 配置的加密导出与导入。
//!
//! 导出文件由明文 JSON 外壳与 AES-256-GCM 密文载荷组成：外壳记录算法与参数，载荷
//! 包含标准模型与所有 Provider 配置。密钥由用户输入的加密字符串经
//! PBKDF2-HMAC-SHA256 派生；外壳内容作为 AEAD 的附加认证数据，因此篡改算法参数
//! 同样会导致解密失败。

mod crypto;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{
        provider::ProviderView,
        provider_names::validate_short_code,
        standard_model::{validate_definition, StandardModel},
    },
    error::{AppError, Result},
};

/// 导出文件扩展名（不含点号），同时用于原生文件对话框的过滤器。
pub const FILE_EXTENSION: &str = "yillm";
/// 文件类型名称，显示在原生文件对话框的下拉框中。
pub const FILE_FORMAT_NAME: &str = "yi-llm";
/// 加密字符串的最小长度（按字符计）。
pub const MIN_PASSPHRASE_CHARS: usize = 8;

/// 载荷格式标识与版本。
const FORMAT: &str = "yi-llm.provider-config";
const FORMAT_VERSION: u32 = 1;
const KDF_ALGORITHM: &str = "pbkdf2-hmac-sha256";
const CIPHER_ALGORITHM: &str = "aes-256-gcm";
/// 解密时接受的迭代次数区间：兼容旧文件，同时避免恶意文件拖垮解密。
const ITERATIONS: std::ops::RangeInclusive<u32> = 10_000..=10_000_000;
/// 合法的上游协议取值。
const PROVIDER_TYPES: [&str; 3] = ["anthropic", "openai_chat", "responses"];

/// 加密文件中的明文载荷：标准模型 + 所有 Provider 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigBundle {
    /// 导出时间（Unix 秒），仅作记录。
    pub exported_at: i64,
    /// 导出方的应用版本，便于排查差异。
    pub app_version: String,
    /// 标准模型（模型目录）快照。
    #[serde(default)]
    pub standard_models: Vec<StandardModel>,
    /// 所有 Provider 及其模型映射。
    #[serde(default)]
    pub providers: Vec<ProviderView>,
}

impl ProviderConfigBundle {
    /// 组装导出载荷并记录导出时间与应用版本。
    pub fn export(providers: Vec<ProviderView>, standard_models: Vec<StandardModel>) -> Self {
        Self {
            exported_at: unix_now(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            standard_models,
            providers,
        }
    }

    /// 载入前的结构自检：所有问题都在写入任何数据之前暴露。
    pub fn validate(&self) -> Result<()> {
        if self.providers.is_empty() && self.standard_models.is_empty() {
            return Err(AppError::validation("配置文件中没有任何模型或 Provider"));
        }
        self.validate_standard_models()?;
        let mut provider_ids = HashSet::new();
        let mut short_codes = HashSet::new();
        for view in &self.providers {
            let provider = &view.provider;
            validate_short_code(&provider.short_code)?;
            if !PROVIDER_TYPES.contains(&provider.provider_type.as_str()) {
                let provider_type = provider.provider_type.as_str();
                return Err(AppError::validation(format!(
                    "Provider 上游协议无效：{provider_type}"
                )));
            }
            let id = provider.id.as_str();
            if !provider_ids.insert(id) {
                return Err(AppError::validation(format!("Provider 标识重复：{id}")));
            }
            let name = provider.name.trim();
            if !short_codes.insert(provider.short_code.to_lowercase()) {
                return Err(AppError::validation(format!("Provider 简写重复：{name}")));
            }
            self.validate_models(view)?;
        }
        Ok(())
    }

    fn validate_standard_models(&self) -> Result<()> {
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for model in &self.standard_models {
            validate_definition(model)?;
            let id = model.id.as_str();
            if !ids.insert(id) {
                return Err(AppError::validation(format!("标准模型标识重复：{id}")));
            }
            if !names.insert((model.name.trim(), model.protocol.as_str())) {
                let name = model.name.trim();
                return Err(AppError::validation(format!(
                    "同一协议下的标准模型名称重复：{name}"
                )));
            }
        }
        Ok(())
    }

    /// 模型映射必须指向文件中提供的标准模型，或自带完整的能力声明。
    fn validate_models(&self, view: &ProviderView) -> Result<()> {
        let provider = &view.provider;
        let name = provider.name.trim();
        let mut upstream_names = HashSet::new();
        for model in &view.models {
            let upstream = model.upstream_model.trim();
            if upstream.is_empty() {
                return Err(AppError::validation(format!(
                    "Provider「{name}」存在没有名称的模型"
                )));
            }
            if !upstream_names.insert(upstream) {
                return Err(AppError::validation(format!(
                    "Provider「{name}」的模型名重复：{upstream}"
                )));
            }
            match &model.standard_model_id {
                Some(id) => {
                    let standard = self
                        .standard_models
                        .iter()
                        .find(|candidate| &candidate.id == id)
                        .ok_or_else(|| {
                            AppError::validation(format!(
                                "Provider「{name}」引用的标准模型 {id} 不在文件中"
                            ))
                        })?;
                    if standard.protocol != provider.provider_type {
                        let standard_name = standard.name.trim();
                        return Err(AppError::validation(format!(
                            "标准模型「{standard_name}」与 Provider「{name}」的上游协议不一致"
                        )));
                    }
                }
                None => model
                    .capabilities
                    .validate(&provider.provider_type)
                    .map_err(|error| {
                        AppError::validation(format!(
                            "Provider「{name}」的模型「{upstream}」：{error}"
                        ))
                    })?,
            }
        }
        Ok(())
    }
}

/// 加密文件外壳：算法与参数明文保存，载荷为密文。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u32,
    kdf: KdfParams,
    cipher: CipherParams,
    /// base64（密文 + 认证标签）。
    payload: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KdfParams {
    algorithm: String,
    iterations: u32,
    /// base64（盐）。
    salt: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CipherParams {
    algorithm: String,
    /// base64（nonce）。
    nonce: String,
}

/// AEAD 的附加认证数据：外壳中除载荷外的全部字段，按声明顺序序列化。
#[derive(Serialize)]
struct Header<'a> {
    format: &'a str,
    version: u32,
    kdf: &'a KdfParams,
    cipher: &'a CipherParams,
}

/// 打包成加密文件内容。
pub fn encode(bundle: &ProviderConfigBundle, passphrase: &str) -> Result<String> {
    encode_with_iterations(bundle, passphrase, crypto::DEFAULT_ITERATIONS)
}

/// [`encode`] 的显式参数版本；迭代次数会写入文件，解密时按文件取值。
fn encode_with_iterations(
    bundle: &ProviderConfigBundle,
    passphrase: &str,
    iterations: u32,
) -> Result<String> {
    bundle.validate()?;
    let passphrase = normalize_passphrase(passphrase)?;
    let salt = crypto::random_bytes(crypto::SALT_LEN);
    let nonce = crypto::random_bytes(crypto::NONCE_LEN);
    let key = crypto::derive_key(passphrase, &salt, iterations);
    let kdf = KdfParams {
        algorithm: KDF_ALGORITHM.into(),
        iterations,
        salt: BASE64.encode(&salt),
    };
    let cipher = CipherParams {
        algorithm: CIPHER_ALGORITHM.into(),
        nonce: BASE64.encode(&nonce),
    };
    let aad = header_bytes(&kdf, &cipher)?;
    let payload = crypto::seal(&key, &nonce, &aad, &serde_json::to_vec(bundle)?)?;
    let envelope = Envelope {
        format: FORMAT.into(),
        version: FORMAT_VERSION,
        kdf,
        cipher,
        payload: BASE64.encode(payload),
    };
    let mut bytes = serde_json::to_vec_pretty(&envelope)?;
    bytes.push(b'\n');
    String::from_utf8(bytes).map_err(|_| AppError::internal("导出内容不是合法的 UTF-8 文本"))
}

/// 解析并解密导出文件内容；加密字符串不正确时返回校验错误。
pub fn decode(text: &str, passphrase: &str) -> Result<ProviderConfigBundle> {
    let passphrase = normalize_passphrase(passphrase)?;
    let envelope: Envelope = serde_json::from_str(text)
        .map_err(|error| AppError::validation(format!("文件格式无法识别：{error}")))?;
    if envelope.format != FORMAT {
        let format = envelope.format.as_str();
        return Err(AppError::validation(format!(
            "不是 yi-llm 导出的配置备份（文件标识：{format}）"
        )));
    }
    if envelope.version != FORMAT_VERSION {
        let version = envelope.version;
        return Err(AppError::validation(format!(
            "不支持的备份版本 {version}，当前支持版本 {FORMAT_VERSION}"
        )));
    }
    if envelope.kdf.algorithm != KDF_ALGORITHM {
        let algorithm = envelope.kdf.algorithm.as_str();
        return Err(AppError::validation(format!(
            "不支持的密钥派生算法：{algorithm}"
        )));
    }
    if envelope.cipher.algorithm != CIPHER_ALGORITHM {
        let algorithm = envelope.cipher.algorithm.as_str();
        return Err(AppError::validation(format!(
            "不支持的加密算法：{algorithm}"
        )));
    }
    if !ITERATIONS.contains(&envelope.kdf.iterations) {
        let iterations = envelope.kdf.iterations;
        return Err(AppError::validation(format!(
            "备份文件中的迭代次数超出支持范围：{iterations}"
        )));
    }
    let salt = decode_base64(&envelope.kdf.salt, "盐")?;
    if salt.len() != crypto::SALT_LEN {
        return Err(AppError::validation("备份文件中的盐长度无效"));
    }
    let nonce = decode_base64(&envelope.cipher.nonce, "nonce")?;
    let sealed = decode_base64(&envelope.payload, "密文")?;
    let key = crypto::derive_key(passphrase, &salt, envelope.kdf.iterations);
    let aad = header_bytes(&envelope.kdf, &envelope.cipher)?;
    let plaintext = crypto::open(&key, &nonce, &aad, &sealed)?;
    let bundle: ProviderConfigBundle = serde_json::from_slice(&plaintext)
        .map_err(|error| AppError::validation(format!("备份内容无法解析：{error}")))?;
    bundle.validate()?;
    Ok(bundle)
}

/// 加密字符串去掉首尾空白后须达到最小长度，避免把弱口令写进密钥派生。
fn normalize_passphrase(passphrase: &str) -> Result<&str> {
    let passphrase = passphrase.trim();
    if passphrase.chars().count() < MIN_PASSPHRASE_CHARS {
        return Err(AppError::validation(format!(
            "加密字符串至少需要 {MIN_PASSPHRASE_CHARS} 个字符"
        )));
    }
    Ok(passphrase)
}

fn header_bytes(kdf: &KdfParams, cipher: &CipherParams) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&Header {
        format: FORMAT,
        version: FORMAT_VERSION,
        kdf,
        cipher,
    })?)
}

fn decode_base64(value: &str, field: &str) -> Result<Vec<u8>> {
    BASE64.decode(value).map_err(|error| {
        AppError::validation(format!("备份文件中的 {field} 不是有效的 base64：{error}"))
    })
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}
