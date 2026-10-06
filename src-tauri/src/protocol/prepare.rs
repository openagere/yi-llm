use crate::domain::{
    capabilities::{effort_levels, Capabilities, Modality, Support},
    provider::Provider,
};
use serde_json::{json, Value};
use std::collections::HashSet;

fn content_modalities(content: &Value, result: &mut HashSet<Modality>) {
    if content.is_string() {
        result.insert(Modality::Text);
    }
    for part in content.as_array().into_iter().flatten() {
        let modality = match part.get("type").and_then(Value::as_str).unwrap_or("") {
            "text" | "input_text" | "output_text" => Some(Modality::Text),
            "image" | "image_url" | "input_image" => Some(Modality::Image),
            "audio" | "input_audio" => Some(Modality::Audio),
            "video" | "video_url" | "input_video" => Some(Modality::Video),
            "file" | "input_file" | "document" => Some(
                if part.pointer("/source/type").and_then(Value::as_str) == Some("text") {
                    Modality::Text
                } else {
                    Modality::Pdf
                },
            ),
            "tool_result" => {
                if let Some(content) = part.get("content") {
                    content_modalities(content, result);
                }
                None
            }
            _ => None,
        };
        if let Some(modality) = modality {
            result.insert(modality);
        }
    }
}

fn set_effort(body: &mut Value, protocol: &str, effort: &str) -> Result<(), String> {
    let (container, field) = match protocol {
        "responses" => ("reasoning", "effort"),
        "anthropic" => ("output_config", "effort"),
        _ => ("", "reasoning_effort"),
    };
    if container.is_empty() {
        body[field] = json!(effort);
    } else {
        if body.get(container).is_none_or(Value::is_null) {
            body[container] = json!({});
        }
        if !body[container].is_object() {
            return Err(format!("{container} 必须是对象"));
        }
        body[container][field] = json!(effort);
    }
    Ok(())
}

/// The smallest level in `levels`, following the canonical effort order.
fn lowest_effort(protocol: &str, levels: &[String]) -> Option<String> {
    effort_levels(protocol)
        .iter()
        .find(|candidate| levels.iter().any(|level| level == *candidate))
        .map(|candidate| (*candidate).to_string())
}

fn remove_effort(body: &mut Value, protocol: &str) {
    let (container, field) = match protocol {
        "responses" => ("reasoning", "effort"),
        "anthropic" => ("output_config", "effort"),
        _ => ("", "reasoning_effort"),
    };
    if container.is_empty() {
        if let Some(object) = body.as_object_mut() {
            object.remove(field);
        }
    } else if let Some(config) = body.get_mut(container).and_then(Value::as_object_mut) {
        config.remove(field);
    }
}

/// How the proxy settles one named effort id a harness-style client sent.
enum SettledEffort {
    /// The request carries this concrete level.
    Level(String),
    /// No reasoning option goes out at all: the client asked for none (`off`/`none`) or
    /// the model serves no level this conversion can carry.
    Cleared,
}

/// Settle a named effort id against the model's own declared levels, or `None`
/// when the id is not one this proxy resolves.
///
/// `default` is the model's configured default level, falling back to the
/// model's smallest level; `off`, and its OpenAI-compatible spelling `none`, ask
/// for no thinking, so no option goes out.
/// Candidates are the model's declared levels narrowed to the ones this
/// conversion carries, so a settled level is always one the upstream accepts.
fn settle_effort(
    capabilities: &Capabilities,
    protocol: &str,
    upstream: &str,
    requested: &str,
) -> Option<SettledEffort> {
    match requested {
        "off" | "none" => Some(SettledEffort::Cleared),
        "default" => {
            let levels = capabilities.efforts_for(protocol, upstream);
            let configured = capabilities
                .effort
                .default
                .as_deref()
                .filter(|configured| levels.iter().any(|level| level.as_str() == *configured));
            match configured
                .map(str::to_owned)
                .or_else(|| lowest_effort(protocol, &levels))
            {
                Some(level) => Some(SettledEffort::Level(level)),
                None => Some(SettledEffort::Cleared),
            }
        }
        _ => None,
    }
}

/// Settle the `default`/`off`/`none` effort ids a harness-style client may send,
/// reporting whether the request's effort was settled here. A settled request
/// skips the validation and default injection below: a concrete level was
/// written, or the option was cleared. Every other value stays for validation.
fn resolve_named_effort(
    body: &mut Value,
    protocol: &str,
    capabilities: &Capabilities,
    upstream: &str,
) -> Result<bool, String> {
    let effort = match protocol {
        "responses" => body.pointer("/reasoning/effort"),
        "anthropic" => body.pointer("/output_config/effort"),
        _ => body.get("reasoning_effort"),
    };
    let Some(Value::String(requested)) = effort else {
        return Ok(false);
    };
    let Some(settled) = settle_effort(capabilities, protocol, upstream, requested) else {
        return Ok(false);
    };
    let level = match settled {
        SettledEffort::Level(level) => Some(level),
        SettledEffort::Cleared => None,
    };
    // Anthropic couples effort with thinking: a fixed budget already decides the
    // effort, so a settled level cannot add one — mirroring the absent-effort guard.
    let level = level.filter(|_| {
        protocol != "anthropic"
            || body.get("thinking").is_none_or(Value::is_null)
            || body.pointer("/thinking/type").and_then(Value::as_str) == Some("adaptive")
    });
    match level {
        Some(level) => set_effort(body, protocol, &level)?,
        None => remove_effort(body, protocol),
    }
    Ok(true)
}

pub fn prepare_request(
    body: &mut Value,
    protocol: &str,
    provider: &mut Provider,
    capabilities: &Capabilities,
) -> Result<(), String> {
    provider.thinking.clear();
    if let Some(allowed) = &capabilities.input_modalities {
        let mut requested = HashSet::new();
        let input = body.get(if protocol == "responses" {
            "input"
        } else {
            "messages"
        });
        if let Some(input) = input {
            if input.is_string() {
                requested.insert(Modality::Text);
            }
            for item in input.as_array().into_iter().flatten() {
                if let Some(content) = item.get("content") {
                    content_modalities(content, &mut requested);
                } else {
                    content_modalities(&json!([item]), &mut requested);
                }
            }
        }
        if let Some(system) = body.get("system") {
            content_modalities(system, &mut requested);
        }
        if body.get("instructions").is_some_and(Value::is_string) {
            requested.insert(Modality::Text);
        }
        if requested.iter().any(|modality| !allowed.contains(modality)) {
            return Err("请求的输入模态不在该模型声明的能力范围内".into());
        }
    }
    if let Some(allowed) = &capabilities.output_modalities {
        if let Some(modalities) = body.get("modalities").and_then(Value::as_array) {
            for value in modalities {
                let modality: Modality =
                    serde_json::from_value(value.clone()).map_err(|_| "未知输出模态")?;
                if !allowed.contains(&modality) {
                    return Err("请求的输出模态不在该模型声明的能力范围内".into());
                }
            }
        }
    }
    let fields = match protocol {
        "responses" => &["max_output_tokens"][..],
        "anthropic" => &["max_tokens"][..],
        _ => &["max_completion_tokens", "max_tokens"][..],
    };
    let limit = capabilities
        .max_output_tokens
        .or(capabilities.context_window);
    for field in fields {
        if let Some(value) = body.get(*field).filter(|value| !value.is_null()) {
            if let Some(limit) = limit {
                let requested = value
                    .as_u64()
                    .filter(|value| *value > 0)
                    .ok_or("最大输出必须是正整数")?;
                if requested > limit as u64 {
                    return Err(format!("最大输出超过模型的 {limit} Token 限制"));
                }
            }
        }
    }
    // Anthropic translation supplies a required output limit when the caller omits it.
    let default_limit = capabilities.max_output_tokens.or_else(|| {
        if provider.provider_type == "anthropic" && protocol != "anthropic" {
            capabilities.context_window.map(|context| {
                context.min(crate::protocol::translate::anthropic::DEFAULT_MAX_TOKENS)
            })
        } else {
            None
        }
    });
    if let Some(limit) = default_limit {
        if fields
            .iter()
            .all(|field| body.get(*field).is_none_or(Value::is_null))
        {
            body[fields[0]] = json!(limit);
        }
    }
    let settled = resolve_named_effort(body, protocol, capabilities, &provider.provider_type)?;
    let effort = match protocol {
        "responses" => body.pointer("/reasoning/effort"),
        "anthropic" => body.pointer("/output_config/effort"),
        _ => body.get("reasoning_effort"),
    }
    .filter(|value| !value.is_null());
    if !settled && capabilities.effort.support != Support::Unknown {
        if let Some(effort) = effort {
            capabilities.validate_effort(Some(effort), protocol, &provider.provider_type)?;
        } else if let Some(default) = &capabilities.effort.default {
            // Adaptive thinking uses effort; an explicit budget remains a separate control.
            if (protocol != "anthropic"
                || body.get("thinking").is_none_or(Value::is_null)
                || body.pointer("/thinking/type").and_then(Value::as_str) == Some("adaptive"))
                && capabilities
                    .efforts_for(protocol, &provider.provider_type)
                    .contains(default)
            {
                set_effort(body, protocol, default)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::capabilities::{effort_levels, Effort};
    fn provider(protocol: &str) -> Provider {
        Provider {
            id: "test".into(),
            provider_type: protocol.into(),
            name: "test".into(),
            short_code: "test".into(),
            base_url: "https://example.com".into(),
            api_key: "".into(),
            enabled: true,
            thinking: "high".into(),
            extra: json!({}),
            is_default: false,
            protocol_support: crate::domain::provider::ProtocolSupport::native(protocol),
        }
    }
    fn caps() -> Capabilities {
        Capabilities {
            input_modalities: Some(vec![Modality::Text, Modality::Image]),
            output_modalities: Some(vec![Modality::Text]),
            max_output_tokens: Some(4096),
            context_window: Some(65536),
            effort: Effort {
                support: Support::Supported,
                levels: vec!["low".into(), "high".into()],
                default: Some("low".into()),
            },
        }
    }
    #[test]
    fn openai_efforts_support_max_and_ultra_and_reject_removed_levels() {
        for protocol in ["responses", "openai_chat"] {
            for level in ["max", "ultra"] {
                let mut value = caps();
                value.effort.levels.push(level.into());
                value.effort.default = Some(level.into());
                assert!(value.validate(protocol).is_ok());
                let mut body = json!({"input":"hello"});
                prepare_request(&mut body, protocol, &mut provider(protocol), &value).unwrap();
                let effort = if protocol == "responses" {
                    body.pointer("/reasoning/effort")
                } else {
                    body.get("reasoning_effort")
                };
                assert_eq!(effort.unwrap(), level);
            }
            for level in ["none", "minimal"] {
                let mut value = caps();
                value.effort.levels.push(level.into());
                assert!(value.validate(protocol).is_err());
            }
        }
    }

    #[test]
    fn request_limits_and_effort_use_each_protocols_native_fields() {
        for (protocol, token_field, effort_path) in [
            ("responses", "max_output_tokens", "/reasoning/effort"),
            ("openai_chat", "max_completion_tokens", "/reasoning_effort"),
            ("anthropic", "max_tokens", "/output_config/effort"),
        ] {
            let mut p = provider(protocol);
            let mut body = json!({"input":"hello","messages":[{"role":"user","content":"hello"}]});
            prepare_request(&mut body, protocol, &mut p, &caps()).unwrap();
            assert_eq!(body[token_field], 4096);
            assert_eq!(body.pointer(effort_path).unwrap(), "low");
            assert!(p.thinking.is_empty());
            body[token_field] = json!(4097);
            assert!(prepare_request(&mut body, protocol, &mut p, &caps()).is_err());
        }
    }
    #[test]
    fn native_multimodal_requests_are_checked_in_messages_and_tool_results() {
        let mut p = provider("anthropic");
        let mut body = json!({"messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"id","content":[{"type":"document","source":{"type":"base64","media_type":"application/pdf","data":"file"}}]}]}]});
        assert!(prepare_request(&mut body, "anthropic", &mut p, &caps()).is_err());
        let mut value = caps();
        value.input_modalities.as_mut().unwrap().push(Modality::Pdf);
        prepare_request(&mut body, "anthropic", &mut p, &value).unwrap();
        let mut body = json!({"messages":[{"role":"user","content":[{"type":"input_audio","input_audio":{"data":"audio","format":"wav"}}]}]});
        assert!(prepare_request(&mut body, "openai_chat", &mut p, &caps()).is_err());
        prepare_request(&mut body, "openai_chat", &mut p, &Capabilities::default()).unwrap();
    }
    #[test]
    fn translated_anthropic_default_output_respects_declared_context() {
        for protocol in ["responses", "openai_chat"] {
            let mut p = provider("anthropic");
            let capabilities = Capabilities {
                context_window: Some(2048),
                ..Default::default()
            };
            let mut body = json!({"model":"actual","input":"hello","messages":[{"role":"user","content":"hello"}]});
            prepare_request(&mut body, protocol, &mut p, &capabilities).unwrap();
            let normalized = crate::protocol::client::normalize_body(protocol, &body).unwrap();
            let request = serde_json::from_value(normalized).unwrap();
            let upstream =
                crate::protocol::translate::anthropic::build_request(&request, &p, "actual")
                    .unwrap();
            assert_eq!(upstream.max_tokens, 2048);
        }
        let mut p = provider("responses");
        let mut body = json!({"input":"hello"});
        prepare_request(
            &mut body,
            "responses",
            &mut p,
            &Capabilities {
                context_window: Some(2048),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(body.get("max_output_tokens").is_none());
    }
    #[test]
    fn unsupported_effort_is_distinct_from_thinking_and_unknown_is_undeclared() {
        let mut p = provider("anthropic");
        let mut value = caps();
        value.effort = Effort {
            support: Support::Unsupported,
            ..Default::default()
        };
        let mut body = json!({"messages":[],"thinking":{"type":"enabled","budget_tokens":1024}});
        prepare_request(&mut body, "anthropic", &mut p, &value).unwrap();
        assert_eq!(body["thinking"]["budget_tokens"], 1024);
        body["output_config"] = json!({"effort":"high"});
        assert!(prepare_request(&mut body, "anthropic", &mut p, &value).is_err());
        prepare_request(&mut body, "anthropic", &mut p, &Capabilities::default()).unwrap();
        let mut body = json!({"thinking":{"type":"enabled","budget_tokens":1024}});
        prepare_request(&mut body, "anthropic", &mut p, &caps()).unwrap();
        assert!(body.get("output_config").is_none());
    }
    #[test]
    fn anthropic_efforts_and_adaptive_defaults_use_native_output_config() {
        let mut value = caps();
        value.effort.levels = effort_levels("anthropic")
            .iter()
            .map(|level| (*level).into())
            .collect();
        assert!(value.validate("anthropic").is_ok());
        for level in effort_levels("anthropic") {
            let mut body = json!({"thinking":{"type":"adaptive"},"output_config":{"effort":level}});
            prepare_request(&mut body, "anthropic", &mut provider("anthropic"), &value).unwrap();
            assert_eq!(body["output_config"]["effort"], *level);
            assert_eq!(body["thinking"]["type"], "adaptive");
        }
        value.effort.default = Some("xhigh".into());
        let mut body = json!({"thinking":{"type":"adaptive"}});
        prepare_request(&mut body, "anthropic", &mut provider("responses"), &value).unwrap();
        assert_eq!(body["output_config"]["effort"], "xhigh");
        value.effort.levels.push("ultra".into());
        assert!(value.validate("anthropic").is_err());
    }
    #[test]
    fn named_default_effort_resolves_to_configured_default_then_lowest() {
        // The model's configured default wins when it survives the conversion.
        let mut value = caps();
        let mut body = json!({"reasoning_effort":"default"});
        prepare_request(
            &mut body,
            "openai_chat",
            &mut provider("openai_chat"),
            &value,
        )
        .unwrap();
        assert_eq!(body["reasoning_effort"], "low");

        // Without a configured default, the lowest supported level is used.
        value.effort.default = None;
        let mut body = json!({"reasoning_effort":"default"});
        prepare_request(
            &mut body,
            "openai_chat",
            &mut provider("openai_chat"),
            &value,
        )
        .unwrap();
        assert_eq!(body["reasoning_effort"], "low");

        // Levels the conversion drops never become the fallback.
        value.effort.levels = vec!["xhigh".into(), "max".into()];
        value.effort.default = Some("low".into());
        let mut body = json!({"reasoning_effort":"default"});
        prepare_request(
            &mut body,
            "openai_chat",
            &mut provider("openai_chat"),
            &value,
        )
        .unwrap();
        assert_eq!(body["reasoning_effort"], "xhigh");

        // Models without effort support drop the field instead of failing.
        value.effort = Effort {
            support: Support::Unsupported,
            ..Default::default()
        };
        let mut body = json!({"reasoning_effort":"default"});
        prepare_request(
            &mut body,
            "openai_chat",
            &mut provider("openai_chat"),
            &value,
        )
        .unwrap();
        assert!(body.get("reasoning_effort").is_none());

        // Undeclared-effort models collapse a named default to no effort too,
        // while explicit levels keep flowing through untouched.
        let mut body = json!({"reasoning_effort":"default"});
        prepare_request(
            &mut body,
            "openai_chat",
            &mut provider("openai_chat"),
            &Capabilities::default(),
        )
        .unwrap();
        assert!(body.get("reasoning_effort").is_none());
        let mut body = json!({"reasoning_effort":"high"});
        prepare_request(
            &mut body,
            "openai_chat",
            &mut provider("openai_chat"),
            &Capabilities::default(),
        )
        .unwrap();
        assert_eq!(body["reasoning_effort"], "high");

        // The responses container path resolves the same way.
        let mut body = json!({"reasoning":{"effort":"default"}});
        prepare_request(&mut body, "responses", &mut provider("responses"), &caps()).unwrap();
        assert_eq!(body.pointer("/reasoning/effort").unwrap(), "low");
    }
    #[test]
    fn anthropic_named_default_resolves_only_while_thinking_stays_adaptive() {
        let value = caps();
        let mut body = json!({"thinking":{"type":"adaptive"},"output_config":{"effort":"default"}});
        prepare_request(&mut body, "anthropic", &mut provider("anthropic"), &value).unwrap();
        assert_eq!(body["output_config"]["effort"], "low");
        let mut body = json!({"thinking":{"type":"enabled","budget_tokens":1024},"output_config":{"effort":"default"}});
        prepare_request(&mut body, "anthropic", &mut provider("anthropic"), &value).unwrap();
        assert!(body.pointer("/output_config/effort").is_none());
        assert_eq!(body["thinking"]["budget_tokens"], 1024);
    }
    #[test]
    fn off_effort_clears_the_option_instead_of_failing() {
        let mut value = caps();
        for requested in ["off", "none"] {
            for protocol in ["responses", "openai_chat"] {
                let mut body = if protocol == "responses" {
                    json!({"reasoning": {"effort": requested}})
                } else {
                    json!({"reasoning_effort": requested})
                };
                prepare_request(&mut body, protocol, &mut provider(protocol), &value).unwrap();
                let present = if protocol == "responses" {
                    body.pointer("/reasoning/effort").is_some()
                } else {
                    body.get("reasoning_effort").is_some()
                };
                assert!(!present, "{requested}/{protocol} should clear the option");
            }
        }
        // An explicit off/none is honoured even when the model configures a default.
        value.effort.default = Some("high".into());
        for requested in ["off", "none"] {
            let mut body = json!({"reasoning_effort": requested});
            prepare_request(
                &mut body,
                "openai_chat",
                &mut provider("openai_chat"),
                &value,
            )
            .unwrap();
            assert!(
                body.get("reasoning_effort").is_none(),
                "{requested} must not gain a default"
            );
        }
        // Anthropic keeps a fixed thinking budget it already carries.
        let mut body = json!({
            "thinking": {"type": "enabled", "budget_tokens": 512},
            "output_config": {"effort": "none"}
        });
        prepare_request(&mut body, "anthropic", &mut provider("anthropic"), &value).unwrap();
        assert!(body.pointer("/output_config/effort").is_none());
        assert_eq!(body["thinking"]["budget_tokens"], 512);
    }
}
