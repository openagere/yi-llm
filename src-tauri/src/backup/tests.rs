//! 导出/导入编解码与加密的单元测试。

use super::*;
use crate::domain::{
    capabilities::Capabilities,
    provider::{ModelMapping, ProtocolSupport, Provider, ProviderView},
};

const PASSPHRASE: &str = "correct horse battery";
/// 测试用的迭代次数：取允许区间下限，避免每个用例都跑生产参数。
const TEST_ITERATIONS: u32 = 10_000;

fn standard_model() -> StandardModel {
    StandardModel {
        id: "claude-sonnet".into(),
        name: "Claude Sonnet".into(),
        protocol: "anthropic".into(),
        brand: "anthropic".into(),
        capabilities: Capabilities::default(),
        provider_count: 2,
    }
}

fn sample() -> ProviderConfigBundle {
    ProviderConfigBundle {
        exported_at: 1_700_000_000,
        app_version: "0.1.0".into(),
        standard_models: vec![standard_model()],
        providers: vec![ProviderView {
            provider: Provider {
                id: "work".into(),
                provider_type: "anthropic".into(),
                name: "工作账号".into(),
                short_code: "work".into(),
                base_url: "https://api.anthropic.com".into(),
                api_key: "sk-ant-secret".into(),
                enabled: true,
                thinking: "high".into(),
                extra: serde_json::json!({ "header": "x" }),
                is_default: true,
                protocol_support: ProtocolSupport::native("anthropic"),
            },
            models: vec![ModelMapping {
                id: 7,
                provider_id: "work".into(),
                exposed_name: "claude-sonnet(work)".into(),
                upstream_model: "claude-sonnet".into(),
                standard_model_id: Some("claude-sonnet".into()),
                capabilities: Capabilities::default(),
                non_standard: false,
            }],
        }],
    }
}

fn encodable(bundle: &ProviderConfigBundle) -> String {
    encode_with_iterations(bundle, PASSPHRASE, TEST_ITERATIONS).unwrap()
}

fn value(bundle: &ProviderConfigBundle) -> serde_json::Value {
    serde_json::to_value(bundle).unwrap()
}

fn envelope(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn round_trip_restores_the_same_configuration() {
    let decoded = decode(&encodable(&sample()), PASSPHRASE).unwrap();
    assert_eq!(value(&decoded), value(&sample()));
}

#[test]
fn export_keeps_secrets_out_of_the_file() {
    let text = encodable(&sample());
    assert!(text.contains(FORMAT));
    assert!(!text.contains("sk-ant-secret"));
    assert!(!text.contains("工作账号"));
}

#[test]
fn a_different_passphrase_cannot_decrypt() {
    let error = decode(&encodable(&sample()), "another passphrase").unwrap_err();
    assert!(error.to_string().contains("解密失败"), "{error}");
}

#[test]
fn payloads_are_bound_to_their_envelope() {
    // 两份文件的盐不同，附加认证数据也不同，密文不能互相搬运。
    let mut forged = envelope(&encodable(&sample()));
    forged["payload"] = envelope(&encodable(&sample()))["payload"].clone();
    let error = decode(&forged.to_string(), PASSPHRASE).unwrap_err();
    assert!(error.to_string().contains("解密失败"), "{error}");
}

#[test]
fn tampered_parameters_are_rejected() {
    let mut forged = envelope(&encodable(&sample()));
    forged["kdf"]["iterations"] = serde_json::json!(TEST_ITERATIONS + 1);
    let error = decode(&forged.to_string(), PASSPHRASE).unwrap_err();
    assert!(error.to_string().contains("解密失败"), "{error}");
}

#[test]
fn foreign_and_malformed_files_are_rejected() {
    assert!(decode("not json", PASSPHRASE).is_err());
    assert!(decode("{}", PASSPHRASE).is_err());

    let mut forged = envelope(&encodable(&sample()));
    forged["format"] = serde_json::json!("other.tool");
    assert!(decode(&forged.to_string(), PASSPHRASE)
        .unwrap_err()
        .to_string()
        .contains("不是 yi-llm"));

    let mut forged = envelope(&encodable(&sample()));
    forged["version"] = serde_json::json!(99);
    assert!(decode(&forged.to_string(), PASSPHRASE)
        .unwrap_err()
        .to_string()
        .contains("版本"));

    let mut forged = envelope(&encodable(&sample()));
    forged["kdf"]["iterations"] = serde_json::json!(10);
    assert!(decode(&forged.to_string(), PASSPHRASE)
        .unwrap_err()
        .to_string()
        .contains("迭代次数"));

    let mut forged = envelope(&encodable(&sample()));
    forged["cipher"]["algorithm"] = serde_json::json!("rot13");
    assert!(decode(&forged.to_string(), PASSPHRASE)
        .unwrap_err()
        .to_string()
        .contains("加密算法"));

    let mut forged = envelope(&encodable(&sample()));
    forged["kdf"]["salt"] = serde_json::json!("bm90LWEtc2FsdA==");
    assert!(decode(&forged.to_string(), PASSPHRASE)
        .unwrap_err()
        .to_string()
        .contains("盐长度"));
}

#[test]
fn short_passphrases_are_rejected() {
    assert!(encode(&sample(), "short").is_err());
    assert!(encode(&sample(), "        ").is_err());
    let error = decode(&encodable(&sample()), "1234567").unwrap_err();
    assert!(error.to_string().contains("至少需要"), "{error}");
}

#[test]
fn unicode_passphrases_round_trip_and_ignore_surrounding_whitespace() {
    let passphrase = "中文口令🔐 空格";
    let text = encode_with_iterations(&sample(), passphrase, TEST_ITERATIONS).unwrap();
    let decoded = decode(&text, " 中文口令🔐 空格 ").unwrap();
    assert_eq!(value(&decoded), value(&sample()));
}

#[test]
fn invalid_bundles_never_reach_the_file() {
    let empty = ProviderConfigBundle {
        exported_at: 0,
        app_version: "0.1.0".into(),
        standard_models: Vec::new(),
        providers: Vec::new(),
    };
    assert!(encode_with_iterations(&empty, PASSPHRASE, TEST_ITERATIONS).is_err());

    let mut dangling = sample();
    dangling.providers[0].models[0].standard_model_id = Some("missing".into());
    assert!(
        encode_with_iterations(&dangling, PASSPHRASE, TEST_ITERATIONS)
            .unwrap_err()
            .to_string()
            .contains("不在文件中")
    );

    let mut duplicated = sample();
    duplicated.providers.push(duplicated.providers[0].clone());
    assert!(encode_with_iterations(&duplicated, PASSPHRASE, TEST_ITERATIONS).is_err());

    let mut mismatched = sample();
    mismatched.providers[0].provider.provider_type = "openai_chat".into();
    assert!(
        encode_with_iterations(&mismatched, PASSPHRASE, TEST_ITERATIONS)
            .unwrap_err()
            .to_string()
            .contains("协议不一致")
    );
}

#[test]
fn derivation_depends_on_passphrase_and_salt() {
    let salt = [7u8; crypto::SALT_LEN];
    let other_salt = [9u8; crypto::SALT_LEN];
    let key = crypto::derive_key(PASSPHRASE, &salt, TEST_ITERATIONS);
    assert_eq!(
        *key,
        *crypto::derive_key(PASSPHRASE, &salt, TEST_ITERATIONS)
    );
    assert_ne!(
        *key,
        *crypto::derive_key(PASSPHRASE, &other_salt, TEST_ITERATIONS)
    );
    assert_ne!(
        *key,
        *crypto::derive_key("another passphrase", &salt, TEST_ITERATIONS)
    );
    assert_eq!(key.len(), crypto::KEY_LEN);
}
