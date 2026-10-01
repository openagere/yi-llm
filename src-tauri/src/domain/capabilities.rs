use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
    Pdf,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    #[default]
    Unknown,
    Unsupported,
    Supported,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Effort {
    pub support: Support,
    pub levels: Vec<String>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Capabilities {
    pub input_modalities: Option<Vec<Modality>>,
    pub output_modalities: Option<Vec<Modality>>,
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub effort: Effort,
}

pub fn effort_levels(protocol: &str) -> &'static [&'static str] {
    match protocol {
        "anthropic" => &["low", "medium", "high", "xhigh", "max", "ultracode"],
        "responses" | "openai_chat" => &["low", "medium", "high", "xhigh", "max", "ultra"],
        _ => &[],
    }
}

impl Capabilities {
    pub fn validate(&self, upstream: &str) -> Result<(), String> {
        for (name, values) in [
            ("输入模态", &self.input_modalities),
            ("输出模态", &self.output_modalities),
        ] {
            if let Some(values) = values {
                if values.is_empty() || values.iter().collect::<HashSet<_>>().len() != values.len()
                {
                    return Err(format!("{name}必须至少选择一项且不能重复"));
                }
            }
        }
        for (name, value) in [
            ("上下文容量", self.context_window),
            ("最大输出", self.max_output_tokens),
        ] {
            if value.is_some_and(|value| value == 0 || value > 100_000_000) {
                return Err(format!("{name}必须是 1 到 100000000 之间的整数 Token 数"));
            }
        }
        if matches!((self.context_window, self.max_output_tokens), (Some(context), Some(output)) if output > context)
        {
            return Err("最大输出不能超过上下文容量".into());
        }
        let allowed = effort_levels(upstream);
        if self.effort.support == Support::Supported {
            if self.effort.levels.is_empty()
                || self.effort.levels.iter().collect::<HashSet<_>>().len()
                    != self.effort.levels.len()
                || self
                    .effort
                    .levels
                    .iter()
                    .any(|level| !allowed.contains(&level.as_str()))
            {
                return Err("Effort 档位为空、重复或不符合上游协议".into());
            }
            if self
                .effort
                .default
                .as_ref()
                .is_some_and(|default| !self.effort.levels.contains(default))
            {
                return Err("默认 Effort 必须在支持的档位中".into());
            }
        } else if !self.effort.levels.is_empty() || self.effort.default.is_some() {
            return Err("只有支持 Effort 的模型可以配置档位和默认值".into());
        }
        Ok(())
    }

    pub fn input_for(&self, client: &str, upstream: &str) -> Vec<Modality> {
        let values = self
            .input_modalities
            .clone()
            .unwrap_or_else(|| vec![Modality::Text]);
        if client == upstream {
            values
        } else {
            values
                .into_iter()
                .filter(|m| matches!(m, Modality::Text | Modality::Image))
                .collect()
        }
    }

    pub fn output_for(&self, client: &str, upstream: &str) -> Vec<Modality> {
        let values = self
            .output_modalities
            .clone()
            .unwrap_or_else(|| vec![Modality::Text]);
        if client == upstream {
            values
        } else {
            values
                .into_iter()
                .filter(|m| *m == Modality::Text)
                .collect()
        }
    }

    pub fn efforts_for(&self, client: &str, upstream: &str) -> Vec<String> {
        if self.effort.support != Support::Supported {
            return Vec::new();
        }
        self.effort
            .levels
            .iter()
            .filter(|level| {
                effort_levels(client).contains(&level.as_str())
                    && effort_levels(upstream).contains(&level.as_str())
            })
            .cloned()
            .collect()
    }

    pub fn validate_effort(
        &self,
        value: Option<&Value>,
        client: &str,
        upstream: &str,
    ) -> Result<(), String> {
        let Some(value) = value.filter(|value| !value.is_null()) else {
            return Ok(());
        };
        if self.effort.support == Support::Unknown {
            return Ok(());
        }
        let effort = value.as_str().ok_or("Effort 必须是文本")?;
        if self.effort.support == Support::Unsupported {
            return Err("该模型不支持设置 Effort".into());
        }
        if !self
            .efforts_for(client, upstream)
            .iter()
            .any(|level| level == effort)
        {
            return Err("请求的 Effort 档位不被模型或协议转换支持".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn validates_limits_modalities_and_protocol_effort_levels() {
        let mut value = caps();
        assert!(value.validate("responses").is_ok());
        value.input_modalities = Some(vec![Modality::Text, Modality::Text]);
        assert!(value.validate("responses").is_err());
        value = caps();
        value.context_window = Some(0);
        assert!(value.validate("responses").is_err());
        value = caps();
        value.max_output_tokens = Some(65537);
        assert!(value.validate("responses").is_err());
        value = caps();
        value.effort.levels.push("xhigh".into());
        assert!(value.validate("responses").is_ok());
        assert!(value.validate("anthropic").is_ok());
        value.effort.levels.push("ultra".into());
        assert!(value.validate("anthropic").is_err());
        value = caps();
        value.effort.support = Support::Unsupported;
        assert!(value.validate("responses").is_err());
    }
    #[test]
    fn catalogs_advertise_only_the_effective_conversion_capabilities() {
        let mut value = caps();
        value.input_modalities = Some(vec![
            Modality::Text,
            Modality::Image,
            Modality::Audio,
            Modality::Pdf,
        ]);
        value.output_modalities = Some(vec![Modality::Text, Modality::Audio]);
        value.effort.levels.push("xhigh".into());
        assert_eq!(
            value.input_for("responses", "openai_chat"),
            vec![Modality::Text, Modality::Image]
        );
        assert_eq!(
            value.output_for("responses", "openai_chat"),
            vec![Modality::Text]
        );
        assert!(value
            .efforts_for("responses", "openai_chat")
            .contains(&"xhigh".into()));
        assert!(value
            .efforts_for("anthropic", "responses")
            .contains(&"xhigh".into()));
        value.effort.levels.extend(["max".into(), "ultra".into()]);
        assert!(value
            .efforts_for("anthropic", "responses")
            .contains(&"max".into()));
        assert!(!value
            .efforts_for("anthropic", "responses")
            .contains(&"ultra".into()));
        assert!(value
            .efforts_for("responses", "anthropic")
            .contains(&"max".into()));
        value.effort.levels = effort_levels("anthropic")
            .iter()
            .map(|level| (*level).into())
            .collect();
        assert!(value
            .efforts_for("anthropic", "anthropic")
            .contains(&"ultracode".into()));
        assert!(!value
            .efforts_for("responses", "anthropic")
            .contains(&"ultracode".into()));
        assert!(!value
            .efforts_for("openai_chat", "anthropic")
            .contains(&"ultracode".into()));
    }
}
