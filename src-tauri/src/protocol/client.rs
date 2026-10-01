use crate::protocol::responses::types::ResponsesRequest;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Wire protocol spoken by a client of the proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientProtocol {
    Responses,
    OpenAiChat,
    Anthropic,
}

impl ClientProtocol {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::OpenAiChat => "openai_chat",
            Self::Anthropic => "anthropic",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "responses" => Some(Self::Responses),
            "openai_chat" => Some(Self::OpenAiChat),
            "anthropic" => Some(Self::Anthropic),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NamespaceToolAlias {
    pub namespace: String,
    pub name: String,
}

pub fn normalize(protocol: &str, body: &Value) -> Result<ResponsesRequest, String> {
    serde_json::from_value(normalize_body(protocol, body)?)
        .map_err(|error| format!("请求格式无效: {error}"))
}

pub fn normalize_body(protocol: &str, body: &Value) -> Result<Value, String> {
    Ok(match protocol {
        "responses" => normalize_responses(body)?,
        "openai_chat" => normalize_chat(body)?,
        "anthropic" => normalize_anthropic(body)?,
        _ => return Err(format!("未知客户端协议: {protocol}")),
    })
}

pub fn validate_client_options(
    client_protocol: &str,
    upstream_protocol: &str,
    body: &Value,
) -> Result<(), String> {
    if body
        .get("stream")
        .is_some_and(|value| !value.is_null() && !value.is_boolean())
    {
        return Err("stream 必须是布尔值".into());
    }
    if body
        .get("tools")
        .is_some_and(|value| !value.is_null() && !value.is_array())
    {
        return Err("tools 必须是数组".into());
    }
    let effort = match client_protocol {
        "responses" => body.pointer("/reasoning/effort"),
        "anthropic" => body.pointer("/output_config/effort"),
        "openai_chat" => body.get("reasoning_effort"),
        _ => None,
    };
    if let Some(value) = effort.filter(|value| !value.is_null()) {
        let effort = value.as_str().ok_or("Effort 必须是文本")?;
        if !crate::domain::capabilities::effort_levels(client_protocol).contains(&effort) {
            return Err(format!("{client_protocol} Effort 档位不受支持: {effort}"));
        }
    }

    if client_protocol == "openai_chat" {
        for (index, message) in body
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if message
                .get("tool_calls")
                .is_some_and(|value| !value.is_null() && !value.is_array())
            {
                return Err(format!("messages[{index}].tool_calls 必须是数组"));
            }
        }
        for field in ["frequency_penalty", "presence_penalty"] {
            if let Some(value) = body.get(field) {
                if value.as_f64() != Some(0.0) {
                    return Err(format!("{field} 不能转换到 {upstream_protocol} 上游"));
                }
            }
        }
        if let Some(value) = body.get("n") {
            if value.as_u64() != Some(1) {
                return Err(format!("n={value} 无法转换：代理目前只生成一个候选响应"));
            }
        }
        if body
            .get("logit_bias")
            .and_then(Value::as_object)
            .is_some_and(|bias| !bias.is_empty())
        {
            return Err(format!("logit_bias 不能转换到 {upstream_protocol} 上游"));
        }
        if body
            .get("logit_bias")
            .is_some_and(|value| !value.is_null() && !value.is_object())
        {
            return Err("logit_bias 必须是对象".into());
        }
        if let Some(value) = body.get("logprobs") {
            let enabled = value.as_bool().ok_or("logprobs 必须是布尔值")?;
            if enabled {
                return Err(format!("logprobs 不能转换到 {upstream_protocol} 上游"));
            }
        }
        if let Some(value) = body.get("top_logprobs") {
            let count = value.as_u64().ok_or("top_logprobs 必须是非负整数")?;
            if count > 0 {
                return Err(format!("top_logprobs 不能转换到 {upstream_protocol} 上游"));
            }
        }
        if body.get("seed").is_some_and(|value| !value.is_null()) {
            return Err(format!("seed 不能转换到 {upstream_protocol} 上游"));
        }
        if body.get("user").is_some_and(|value| !value.is_null()) {
            return Err(format!("user 不能转换到 {upstream_protocol} 上游"));
        }
        for field in ["functions", "function_call"] {
            if body.get(field).is_some_and(|value| !value.is_null()) {
                return Err(format!(
                    "旧版 Chat Completions 字段 {field} 尚不支持跨协议转换，请改用 tools/tool_calls"
                ));
            }
        }
        for field in ["audio", "prediction"] {
            if body.get(field).is_some_and(|value| !value.is_null()) {
                return Err(format!("{field} 不能转换到 {upstream_protocol} 上游"));
            }
        }
        if let Some(modalities) = body.get("modalities") {
            if !modalities
                .as_array()
                .is_some_and(|items| items.iter().all(|item| item.as_str() == Some("text")))
            {
                return Err(format!("modalities 不能转换到 {upstream_protocol} 上游"));
            }
        }
        if body
            .get("parallel_tool_calls")
            .is_some_and(|value| !value.is_boolean())
        {
            return Err("parallel_tool_calls 必须是布尔值".into());
        }
    }

    if client_protocol == "anthropic" && upstream_protocol != "anthropic" {
        if body.get("top_k").is_some_and(|value| !value.is_null()) {
            return Err(format!(
                "Anthropic top_k 不能转换到 {upstream_protocol} 上游"
            ));
        }
        if let Some(thinking) = body.get("thinking").filter(|value| !value.is_null()) {
            let kind = thinking.get("type").and_then(Value::as_str).unwrap_or("");
            match kind {
                "enabled" => {
                    if thinking
                        .get("budget_tokens")
                        .and_then(Value::as_u64)
                        .filter(|budget| *budget > 0)
                        .is_none()
                    {
                        return Err("thinking.budget_tokens 缺失或无效".into());
                    }
                }
                "adaptive" | "disabled" => {}
                _ => return Err(format!("Anthropic thinking.type 无法转换: {kind:?}")),
            }
        }
        if let Some(output_config) = body.get("output_config") {
            if !output_config.is_object() {
                return Err("output_config 必须是对象".into());
            }
            if let Some(format) = output_config.get("format") {
                if format.get("type").and_then(Value::as_str) != Some("json_schema") {
                    return Err("Anthropic output_config.format 只支持 json_schema".into());
                }
                if format.get("schema").is_none() {
                    return Err("output_config.format.schema 缺失".into());
                }
            }
        }
        if let Some(stop_sequences) = body.get("stop_sequences") {
            if !stop_sequences.as_array().is_some_and(|values| {
                values
                    .iter()
                    .all(|value| value.as_str().is_some_and(|text| !text.is_empty()))
            }) {
                return Err("stop_sequences 必须是文本数组".into());
            }
        }
    }
    Ok(())
}

pub fn validate_for_upstream(upstream_protocol: &str, body: &Value) -> Result<(), String> {
    if let Some(metadata) = body.get("metadata").filter(|value| !value.is_null()) {
        let metadata = metadata.as_object().ok_or("metadata 必须是对象")?;
        if metadata.len() > 16
            || metadata.iter().any(|(key, value)| {
                key.chars().count() > 64
                    || value.as_str().is_none_or(|text| text.chars().count() > 512)
            })
        {
            return Err(
                "metadata 最多包含 16 个文本字段，键不能超过 64 字符，值不能超过 512 字符".into(),
            );
        }
    }
    if let Some(value) = body
        .pointer("/reasoning/effort")
        .filter(|value| !value.is_null())
    {
        let effort = value.as_str().ok_or("reasoning.effort 必须是文本")?;
        if !crate::domain::capabilities::effort_levels(upstream_protocol).contains(&effort) {
            return Err(format!(
                "reasoning.effort 不支持转换到 {upstream_protocol} 上游: {effort}"
            ));
        }
    }
    if upstream_protocol == "responses" {
        if body.get("stop").is_some_and(|value| !value.is_null()) {
            return Err("stop 不能转换到 Responses 上游".into());
        }
        return Ok(());
    }
    if !matches!(upstream_protocol, "anthropic" | "openai_chat") {
        return Err(format!("未知上游协议: {upstream_protocol}"));
    }

    let input = body
        .get("input")
        .ok_or_else(|| "请求缺少 input".to_string())?;
    if !input.is_string() {
        let items = input
            .as_array()
            .ok_or_else(|| "input 必须是文本或消息数组".to_string())?;
        for (index, item) in items.iter().enumerate() {
            let path = format!("input[{index}]");
            match item.get("type").and_then(Value::as_str).unwrap_or("") {
                "message" => {
                    let role = item.get("role").and_then(Value::as_str).unwrap_or("");
                    if !matches!(role, "system" | "developer" | "user" | "assistant") {
                        return Err(format!("{path}.role 不受支持: {role}"));
                    }
                    let Some(content) = item.get("content") else {
                        continue;
                    };
                    let Some(parts) = content.as_array() else {
                        if content.is_string() {
                            continue;
                        }
                        return Err(format!("{path}.content 必须是文本或内容数组"));
                    };
                    for (part_index, part) in parts.iter().enumerate() {
                        let part_path = format!("{path}.content[{part_index}]");
                        match part.get("type").and_then(Value::as_str).unwrap_or("") {
                            "input_text" | "text" | "output_text" => {}
                            "input_image" => {
                                if role != "user" {
                                    return Err(format!("{part_path} 图片只支持 user 消息"));
                                }
                                let image_url = part
                                    .get("image_url")
                                    .and_then(Value::as_str)
                                    .filter(|url| !url.is_empty())
                                    .ok_or_else(|| format!("{part_path}.image_url 缺失或为空"))?;
                                if upstream_protocol == "anthropic"
                                    && (!image_url.starts_with("data:image/")
                                        || !image_url.contains(";base64,"))
                                {
                                    return Err(format!(
                                        "{part_path} 的图片无法转换：Anthropic 上游目前要求 base64 data URL"
                                    ));
                                }
                            }
                            kind => {
                                return Err(format!(
                                    "{part_path} 内容类型 {kind:?} 无法转换到 {upstream_protocol} 上游"
                                ));
                            }
                        }
                    }
                }
                "function_call" => {
                    for field in ["call_id", "name", "arguments"] {
                        if item
                            .get(field)
                            .and_then(Value::as_str)
                            .is_none_or(str::is_empty)
                        {
                            return Err(format!("{path}.{field} 缺失或为空"));
                        }
                    }
                }
                "function_call_output" => {
                    if item
                        .get("call_id")
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty)
                    {
                        return Err(format!("{path}.call_id 缺失或为空"));
                    }
                }
                "reasoning" => {
                    for field in ["content", "summary"] {
                        if let Some(parts) = item.get(field).and_then(Value::as_array) {
                            for (part_index, part) in parts.iter().enumerate() {
                                let kind = part.get("type").and_then(Value::as_str).unwrap_or("");
                                if !matches!(kind, "reasoning_text" | "summary_text") {
                                    return Err(format!(
                                        "{path}.{field}[{part_index}] 类型 {kind:?} 无法转换到 {upstream_protocol} 上游"
                                    ));
                                }
                            }
                        }
                    }
                }
                kind => {
                    return Err(format!(
                        "{path}.type {kind:?} 无法转换到 {upstream_protocol} 上游；文件输入和未知输入类型不支持跨协议转换"
                    ));
                }
            }
        }
    }

    if let Some(instructions) = body.get("instructions") {
        if !instructions.is_string() {
            let parts = instructions
                .as_array()
                .ok_or("instructions 必须是文本或文本块数组")?;
            for (index, part) in parts.iter().enumerate() {
                let kind = part.get("type").and_then(Value::as_str).unwrap_or("text");
                if !matches!(kind, "text" | "input_text" | "output_text") {
                    return Err(format!("instructions[{index}] 内容类型不支持: {kind}"));
                }
                if part.get("text").and_then(Value::as_str).is_none() {
                    return Err(format!("instructions[{index}].text 缺失"));
                }
            }
        }
    }

    if let Some(stop) = body.get("stop") {
        let valid = stop.as_str().is_some_and(|value| !value.is_empty())
            || stop.as_array().is_some_and(|values| {
                !values.is_empty()
                    && values
                        .iter()
                        .all(|value| value.as_str().is_some_and(|text| !text.is_empty()))
            });
        if !valid {
            return Err("stop 必须是非空文本或非空文本数组".into());
        }
    }
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        for (index, tool) in tools.iter().enumerate() {
            let kind = tool.get("type").and_then(Value::as_str).unwrap_or("");
            // `web_search` / `web_search_preview` are hosted built-in tools:
            // the canonical format models them, and the upstream builders
            // omit them for anthropic/openai_chat upstreams (they cannot be
            // hosted by third-party endpoints). Anything else is not a known
            // tool shape and fails here rather than being silently dropped.
            if !matches!(
                kind,
                "function" | "custom" | "web_search" | "web_search_preview"
            ) {
                return Err(format!(
                    "tools[{index}].type {kind:?} 无法转换到 {upstream_protocol} 上游"
                ));
            }
        }
    }

    if let Some(choice) = body.get("tool_choice") {
        if let Some(mode) = choice.as_str() {
            if !matches!(mode, "auto" | "none" | "required") {
                return Err(format!("tool_choice 模式不受支持: {mode}"));
            }
        } else {
            let kind = choice.get("type").and_then(Value::as_str).unwrap_or("");
            match kind {
                "function" | "custom" => {
                    if choice
                        .get("name")
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty)
                    {
                        return Err("tool_choice.name 缺失或为空".into());
                    }
                }
                "none" | "required" => {}
                _ => return Err(format!("tool_choice.type 不受支持: {kind:?}")),
            }
        }
    }

    if let Some(format) = body.pointer("/text/format") {
        match format.get("type").and_then(Value::as_str).unwrap_or("") {
            "json_schema" if format.get("schema").is_none() => {
                return Err("text.format.schema 缺失".into());
            }
            "json_schema" => {}
            "json_object" if upstream_protocol == "openai_chat" => {}
            "json_object" => {
                return Err(format!(
                    "text.format.type json_object 无法转换到 {upstream_protocol} 上游"
                ));
            }
            kind => {
                return Err(format!(
                    "text.format.type {kind:?} 无法转换到 {upstream_protocol} 上游"
                ));
            }
        }
    }

    Ok(())
}

fn normalize_responses(body: &Value) -> Result<Value, String> {
    let mut out = body.clone();
    let aliases = namespace_tool_aliases(body);
    if let Some(tools) = out.get("tools").and_then(Value::as_array) {
        for (namespace_index, namespace) in tools.iter().enumerate() {
            if namespace.get("type").and_then(Value::as_str) != Some("namespace") {
                continue;
            }
            namespace
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("tools[{namespace_index}] namespace 缺少 name"))?;
            let nested = namespace
                .get("tools")
                .or_else(|| namespace.get("functions"))
                .and_then(Value::as_array)
                .ok_or_else(|| format!("tools[{namespace_index}] namespace 缺少 tools 数组"))?;
            for (tool_index, tool) in nested.iter().enumerate() {
                let definition = tool.get("function").unwrap_or(tool);
                definition
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("tools[{namespace_index}].tools[{tool_index}] 缺少 name")
                    })?;
            }
        }
    }

    if let Some(items) = out.get_mut("input").and_then(Value::as_array_mut) {
        for item in items {
            if item.get("type").and_then(Value::as_str) == Some("message") {
                if let Some(text) = item
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                {
                    let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
                    item["content"] = json!([{
                        "type":if role == "assistant" {"output_text"} else {"input_text"},
                        "text":text
                    }]);
                }
            }
            match item.get("type").and_then(Value::as_str) {
                Some("custom_tool_call") => {
                    let input = item
                        .get("input")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    *item = json!({
                        "type":"function_call",
                        "call_id":item.get("call_id").and_then(Value::as_str).unwrap_or(""),
                        "name":item.get("name").and_then(Value::as_str).unwrap_or(""),
                        "arguments":json!({"input":input}).to_string(),
                    });
                }
                Some("custom_tool_call_output") => {
                    *item = json!({
                        "type":"function_call_output",
                        "call_id":item.get("call_id").and_then(Value::as_str).unwrap_or(""),
                        "output":item.get("output").cloned().unwrap_or(Value::Null),
                    });
                }
                _ => {}
            }
            if item.get("type").and_then(Value::as_str) == Some("function_call") {
                let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
                let namespace_name = item
                    .get("namespace")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if let Some(internal) = aliases.iter().find_map(|(internal, alias)| {
                    let exact = alias.namespace == namespace_name && alias.name == name;
                    let qualified = namespace_name.is_empty()
                        && format!("{}.{}", alias.namespace, alias.name) == name;
                    (exact || qualified).then_some(internal)
                }) {
                    item["name"] = json!(internal);
                    if let Some(object) = item.as_object_mut() {
                        object.remove("namespace");
                    }
                }
            }
        }
    }

    if let Some(tools) = out.get_mut("tools").and_then(Value::as_array_mut) {
        let mut flattened = Vec::new();
        for (namespace_index, tool) in tools.iter().enumerate() {
            if tool.get("type").and_then(Value::as_str) != Some("namespace") {
                flattened.push(tool.clone());
                continue;
            }
            let Some(nested) = tool
                .get("tools")
                .or_else(|| tool.get("functions"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for (tool_index, nested_tool) in nested.iter().enumerate() {
                let definition = nested_tool.get("function").unwrap_or(nested_tool);
                let internal_name = format!("__yi_llm_namespace_{namespace_index}_{tool_index}");
                flattened.push(json!({
                    "type":"function",
                    "name":internal_name,
                    "description":definition.get("description"),
                    "parameters":definition.get("inputSchema").or_else(|| definition.get("parameters")).cloned().unwrap_or_else(|| json!({"type":"object"})),
                }));
            }
        }
        *tools = flattened;
    }

    if let Some(choice) = out.get_mut("tool_choice") {
        if let Some(name) = choice.get("name").and_then(Value::as_str) {
            let namespace_name = choice
                .get("namespace")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(internal) = aliases.iter().find_map(|(internal, alias)| {
                let exact = alias.namespace == namespace_name && alias.name == name;
                let qualified = namespace_name.is_empty()
                    && format!("{}.{}", alias.namespace, alias.name) == name;
                (exact || qualified).then_some(internal)
            }) {
                choice["name"] = json!(internal);
                if let Some(object) = choice.as_object_mut() {
                    object.remove("namespace");
                }
            }
        }
    }
    Ok(out)
}

pub fn namespace_tool_aliases(body: &Value) -> HashMap<String, NamespaceToolAlias> {
    let mut aliases = HashMap::new();
    for (namespace_index, namespace) in body
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if namespace.get("type").and_then(Value::as_str) != Some("namespace") {
            continue;
        }
        let Some(namespace_name) = namespace.get("name").and_then(Value::as_str) else {
            continue;
        };
        for (tool_index, tool) in namespace
            .get("tools")
            .or_else(|| namespace.get("functions"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let definition = tool.get("function").unwrap_or(tool);
            if let Some(name) = definition.get("name").and_then(Value::as_str) {
                aliases.insert(
                    format!("__yi_llm_namespace_{namespace_index}_{tool_index}"),
                    NamespaceToolAlias {
                        namespace: namespace_name.to_owned(),
                        name: name.to_owned(),
                    },
                );
            }
        }
    }
    aliases
}

fn normalize_chat(body: &Value) -> Result<Value, String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("Chat Completions 请求缺少 messages 数组")?;
    let mut instructions = Vec::new();
    let mut input = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        let content = message.get("content");
        if matches!(role, "system" | "developer") {
            if let Some(text) = content_text(content)? {
                instructions.push(text);
            }
            continue;
        }
        if role == "tool" {
            let call_id = message
                .get("tool_call_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("messages[{index}].tool_call_id 缺失或为空"))?;
            input.push(json!({
                "type":"function_call_output",
                "call_id": call_id,
                "output": text_content(content)
                    .map_err(|error| format!("messages[{index}].content: {error}"))?,
            }));
            continue;
        }
        if role != "user" && role != "assistant" {
            return Err(format!("messages[{index}] 的 role 不受支持: {role}"));
        }
        let normalized_content = normalize_chat_content(index, role, content)?;
        if !normalized_content.is_empty() {
            input.push(json!({
                "type":"message", "role":role,
                "content":normalized_content
            }));
        }
        if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                let function = call.get("function").unwrap_or(&Value::Null);
                let call_id = call
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("messages[{index}].tool_calls 缺少有效 id"))?;
                let name = function
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("messages[{index}].tool_calls 缺少函数名称"))?;
                input.push(json!({
                    "type":"function_call",
                    "call_id":call_id,
                    "name":name,
                    "arguments":function.get("arguments").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| function.get("arguments").cloned().unwrap_or_else(|| json!({})).to_string()),
                }));
            }
        }
    }
    let mut out = json!({
        "model":body.get("model").and_then(Value::as_str).unwrap_or(""),
        "input":input,
        "stream":body.get("stream").and_then(Value::as_bool).unwrap_or(false),
        "store":false,
    });
    if !instructions.is_empty() {
        out["instructions"] = json!(instructions.join("\n\n"));
    }
    for key in ["temperature", "top_p"] {
        if let Some(value) = body.get(key) {
            out[key] = value.clone();
        }
    }
    for key in ["stop", "parallel_tool_calls", "metadata"] {
        if let Some(value) = body.get(key) {
            out[key] = value.clone();
        }
    }
    if let Some(value) = body
        .get("max_completion_tokens")
        .or_else(|| body.get("max_tokens"))
    {
        out["max_output_tokens"] = value.clone();
    }
    if let Some(effort) = body.get("reasoning_effort") {
        out["reasoning"] = json!({"effort":effort});
    }
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let mut normalized = Vec::with_capacity(tools.len());
        for (index, tool) in tools.iter().enumerate() {
            let function = match tool.get("type").and_then(Value::as_str) {
                Some("function") => tool
                    .get("function")
                    .ok_or_else(|| format!("tools[{index}].function 缺失"))?,
                Some(kind) => {
                    return Err(format!("Chat Completions 工具类型不支持协议转换: {kind}"))
                }
                None => tool,
            };
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("tools[{index}] 缺少函数名称"))?;
            normalized.push(json!({
                "type":"function",
                "name":name,
                "description":function.get("description"),
                "parameters":function.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object"})),
            }));
        }
        out["tools"] = Value::Array(normalized);
    }
    if let Some(choice) = body.get("tool_choice") {
        out["tool_choice"] = match choice.as_str() {
            Some("required") => json!("required"),
            Some(value) => json!(value),
            None if choice.get("function").is_some() => json!({
                "type":"function", "name":choice["function"]["name"]
            }),
            None => choice.clone(),
        };
    }
    if let Some(format) = body.get("response_format") {
        match format.get("type").and_then(Value::as_str).unwrap_or("") {
            "json_schema" => {
                let schema = format.get("json_schema").unwrap_or(format);
                let schema_value = schema
                    .get("schema")
                    .ok_or("response_format.json_schema.schema 缺失")?;
                out["text"] = json!({"format":{
                    "type":"json_schema",
                    "name":schema.get("name").and_then(Value::as_str).unwrap_or("structured_output"),
                    "schema":schema_value,
                }});
            }
            "json_object" => {
                out["text"] = json!({"format":{"type":"json_object"}});
            }
            "text" => {}
            kind => {
                return Err(format!(
                    "Chat Completions response_format 类型不支持: {kind}"
                ))
            }
        }
    }
    Ok(out)
}

fn normalize_anthropic(body: &Value) -> Result<Value, String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("Anthropic Messages 请求缺少 messages 数组")?;
    let mut input = Vec::new();
    let mut instructions = Vec::new();
    if let Some(system) = body.get("system") {
        instructions.push(anthropic_instruction_text(system, "system")?);
    }
    for (index, message) in messages.iter().enumerate() {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        if matches!(role, "system" | "developer") {
            let content = message
                .get("content")
                .ok_or_else(|| format!("messages[{index}].content 缺失"))?;
            instructions.push(anthropic_instruction_text(
                content,
                &format!("messages[{index}].content"),
            )?);
            continue;
        }
        if !matches!(role, "user" | "assistant") {
            return Err(format!("messages[{index}] 的 role 无效"));
        }
        let Some(blocks) = message.get("content") else {
            continue;
        };
        if let Some(text) = blocks.as_str() {
            input.push(json!({"type":"message","role":role,"content":[{"type":if role == "assistant" {"output_text"} else {"input_text"},"text":text}]}));
            continue;
        }
        let Some(blocks) = blocks.as_array() else {
            return Err(format!("messages[{index}].content 必须是文本或数组"));
        };
        let mut text = String::new();
        for (block_index, block) in blocks.iter().enumerate() {
            match block.get("type").and_then(Value::as_str).unwrap_or("") {
                "text" => text.push_str(
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!("messages[{index}].content[{block_index}] 缺少 text")
                        })?,
                ),
                "tool_use" => {
                    flush_message(&mut input, role, &mut text);
                    input.push(json!({"type":"function_call","call_id":block.get("id").and_then(Value::as_str).unwrap_or(""),"name":block.get("name").and_then(Value::as_str).unwrap_or(""),"arguments":block.get("input").cloned().unwrap_or_else(|| json!({})).to_string()}));
                }
                "tool_result" => {
                    flush_message(&mut input, role, &mut text);
                    let tool_use_id = block
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            format!("messages[{index}].content[{block_index}] 缺少 tool_use_id")
                        })?;
                    let content = block.get("content").ok_or_else(|| {
                        format!("messages[{index}].content[{block_index}] 缺少 content")
                    })?;
                    input.push(json!({
                        "type":"function_call_output",
                        "call_id":tool_use_id,
                        "output":text_content(Some(content)).map_err(|error| format!("messages[{index}].content[{block_index}]: {error}"))?
                    }));
                }
                "thinking" => {
                    flush_message(&mut input, role, &mut text);
                    input.push(json!({"type":"reasoning","summary":[{"type":"summary_text","text":block.get("thinking").and_then(Value::as_str).unwrap_or("")}],"encrypted_content":block.get("signature").and_then(Value::as_str)}));
                }
                "redacted_thinking" => {
                    flush_message(&mut input, role, &mut text);
                    input.push(json!({"type":"reasoning","encrypted_content":block.get("data").and_then(Value::as_str)}));
                }
                "image" => {
                    if role != "user" {
                        return Err(format!(
                            "messages[{index}].content[{block_index}] 图片只支持 user 消息"
                        ));
                    }
                    flush_message(&mut input, role, &mut text);
                    input.push(json!({
                        "type":"message",
                        "role":"user",
                        "content":[{"type":"input_image","image_url":anthropic_image_url(block).map_err(|error| format!("messages[{index}].content[{block_index}]: {error}"))?}]
                    }));
                }
                kind => {
                    return Err(format!(
                        "messages[{index}].content[{block_index}] Anthropic content 类型不支持协议转换: {kind}"
                    ))
                }
            }
        }
        flush_message(&mut input, role, &mut text);
    }
    let mut out = json!({
        "model":body.get("model").and_then(Value::as_str).unwrap_or(""),
        "input":input,
        "stream":body.get("stream").and_then(Value::as_bool).unwrap_or(false),
        "store":false,
    });
    instructions.retain(|text| !text.is_empty());
    if !instructions.is_empty() {
        out["instructions"] = json!(instructions.join("\n\n"));
    }
    for key in ["temperature", "top_p"] {
        if let Some(value) = body.get(key) {
            out[key] = value.clone();
        }
    }
    if let Some(stop_sequences) = body.get("stop_sequences") {
        out["stop"] = stop_sequences.clone();
    }
    if let Some(value) = body.get("max_tokens") {
        out["max_output_tokens"] = value.clone();
    }
    if let Some(metadata) = body.get("metadata") {
        out["metadata"] = metadata.clone();
    }
    if let Some(effort) = body
        .pointer("/output_config/effort")
        .filter(|value| !value.is_null())
    {
        // Explicit effort takes precedence over the legacy thinking budget heuristic.
        out["reasoning"] = json!({"effort":effort});
    } else if body.pointer("/thinking/type").and_then(Value::as_str) == Some("enabled") {
        let budget = body
            .pointer("/thinking/budget_tokens")
            .and_then(Value::as_u64)
            .filter(|budget| *budget > 0)
            .ok_or("thinking.budget_tokens 缺失或无效")?;
        let effort = if budget < 8192 {
            "low"
        } else if budget < 20000 {
            "medium"
        } else {
            "high"
        };
        out["reasoning"] = json!({"effort":effort});
    }
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let mut normalized = Vec::with_capacity(tools.len());
        for (index, tool) in tools.iter().enumerate() {
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("tools[{index}] 缺少函数名称，内置工具不支持协议转换"))?;
            normalized.push(json!({
                "type":"function",
                "name":name,
                "description":tool.get("description"),
                "parameters":tool.get("input_schema").cloned().unwrap_or_else(|| json!({"type":"object"})),
            }));
        }
        out["tools"] = Value::Array(normalized);
    }
    if let Some(choice) = body.get("tool_choice") {
        out["tool_choice"] = match choice.get("type").and_then(Value::as_str) {
            Some("any") => json!("required"),
            Some("tool") => {
                let name = choice
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .ok_or("tool_choice.type=tool 时必须提供 name")?;
                json!({"type":"function","name":name})
            }
            Some("auto") => json!("auto"),
            Some(kind) => return Err(format!("Anthropic tool_choice 类型不支持: {kind}")),
            None => return Err("Anthropic tool_choice 缺少 type".into()),
        };
        if let Some(disable) = choice.get("disable_parallel_tool_use") {
            let disable = disable
                .as_bool()
                .ok_or("tool_choice.disable_parallel_tool_use 必须是布尔值")?;
            out["parallel_tool_calls"] = json!(!disable);
        }
    }
    if let Some(format) = body
        .get("output_config")
        .and_then(|value| value.get("format"))
    {
        match format.get("type").and_then(Value::as_str).unwrap_or("") {
            "json_schema" => {
                let schema = format
                    .get("schema")
                    .ok_or("output_config.format.schema 缺失")?;
                out["text"] = json!({"format":{
                    "type":"json_schema",
                    "name":format.get("name").and_then(Value::as_str).unwrap_or("structured_output"),
                    "schema":schema,
                }});
            }
            kind => return Err(format!("Anthropic output_config.format 类型不支持: {kind}")),
        }
    }
    Ok(out)
}

fn anthropic_instruction_text(content: &Value, path: &str) -> Result<String, String> {
    if let Some(text) = content.as_str() {
        return Ok(text.to_owned());
    }
    content
        .as_array()
        .ok_or_else(|| format!("{path} 必须是文本或文本块数组"))?
        .iter()
        .enumerate()
        .map(|(index, block)| {
            if block.get("type").and_then(Value::as_str).unwrap_or("text") != "text" {
                return Err(format!("{path}[{index}] 指令只支持文本内容"));
            }
            block
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{path}[{index}].text 缺失"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|texts| texts.join("\n\n"))
}

fn content_text(content: Option<&Value>) -> Result<Option<String>, String> {
    let Some(content) = content else {
        return Ok(None);
    };
    if let Some(text) = content.as_str() {
        return Ok(Some(text.to_owned()));
    }
    let Some(parts) = content.as_array() else {
        return Ok(None);
    };
    let mut text = String::new();
    for part in parts {
        match part.get("type").and_then(Value::as_str).unwrap_or("") {
            "text" | "input_text" => {
                text.push_str(part.get("text").and_then(Value::as_str).unwrap_or(""))
            }
            "image_url" | "input_image" => return Err("system/developer 消息不支持图片输入".into()),
            kind => return Err(format!("Chat Completions 内容类型不支持协议转换: {kind}")),
        }
    }
    Ok(Some(text))
}

fn normalize_chat_content(
    message_index: usize,
    role: &str,
    content: Option<&Value>,
) -> Result<Vec<Value>, String> {
    let Some(content) = content else {
        return Ok(Vec::new());
    };
    if content.is_null() {
        return Ok(Vec::new());
    }
    if let Some(text) = content.as_str() {
        if text.is_empty() {
            return Ok(Vec::new());
        }
        return Ok(vec![json!({
            "type":if role == "assistant" {"output_text"} else {"input_text"},
            "text":text
        })]);
    }
    let parts = content
        .as_array()
        .ok_or_else(|| format!("messages[{message_index}].content 必须是文本或数组"))?;
    let mut normalized = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        match part.get("type").and_then(Value::as_str).unwrap_or("") {
            "text" | "input_text" | "output_text" => {
                let text = part.get("text").and_then(Value::as_str).unwrap_or("");
                normalized.push(json!({
                    "type":if role == "assistant" {"output_text"} else {"input_text"},
                    "text":text
                }));
            }
            "image_url" | "input_image" => {
                if role != "user" {
                    return Err(format!(
                        "messages[{message_index}].content[{index}] 图片只支持 user 消息"
                    ));
                }
                let image = part.get("image_url").or_else(|| part.get("url"));
                let image_url = image
                    .and_then(|value| value.as_str().or_else(|| value.get("url")?.as_str()))
                    .filter(|url| !url.is_empty())
                    .ok_or_else(|| {
                        format!("messages[{message_index}].content[{index}] 缺少图片 URL")
                    })?;
                let detail = image
                    .and_then(|value| value.get("detail"))
                    .or_else(|| part.get("detail"));
                let mut normalized_image = json!({
                    "type":"input_image",
                    "image_url":image_url
                });
                if let Some(detail) = detail {
                    normalized_image["detail"] = detail.clone();
                }
                normalized.push(normalized_image);
            }
            kind => {
                return Err(format!(
                    "messages[{message_index}].content[{index}] 内容类型不支持协议转换: {kind}"
                ))
            }
        }
    }
    Ok(normalized)
}

fn anthropic_image_url(block: &Value) -> Result<String, String> {
    let source = block.get("source").ok_or("Anthropic 图片缺少 source")?;
    match source.get("type").and_then(Value::as_str).unwrap_or("") {
        "base64" => {
            let media_type = source
                .get("media_type")
                .and_then(Value::as_str)
                .filter(|value| value.starts_with("image/") && !value.contains([';', ',']))
                .ok_or("Anthropic 图片 media_type 无效")?;
            let data = source
                .get("data")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or("Anthropic 图片 data 为空")?;
            Ok(format!("data:{media_type};base64,{data}"))
        }
        "url" => source
            .get("url")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| "Anthropic 图片 URL 为空".into()),
        _ => Err("Anthropic 图片 source 类型不支持协议转换".into()),
    }
}

fn text_content(content: Option<&Value>) -> Result<Value, String> {
    let Some(content) = content else {
        return Ok(Value::Null);
    };
    if content.is_string() {
        return Ok(content.clone());
    }
    if let Some(blocks) = content.as_array() {
        let mut text = String::new();
        for part in blocks {
            match part.get("type").and_then(Value::as_str).unwrap_or("text") {
                "text" | "input_text" | "output_text" => {
                    text.push_str(part.get("text").and_then(Value::as_str).unwrap_or(""));
                }
                "image" | "image_url" | "input_image" => {
                    return Err("图片输入暂不支持协议转换".into())
                }
                kind => return Err(format!("工具结果内容类型不支持协议转换: {kind}")),
            }
        }
        return Ok(json!(text));
    }
    Ok(json!(content.to_string()))
}

fn flush_message(input: &mut Vec<Value>, role: &str, text: &mut String) {
    if text.is_empty() {
        return;
    }
    input.push(json!({"type":"message","role":role,"content":[{"type":if role == "assistant" {"output_text"} else {"input_text"},"text":std::mem::take(text)}]}));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn anthropic_explicit_effort_takes_precedence_over_thinking() {
        for upstream in ["responses", "openai_chat"] {
            for thinking in [
                json!({"type":"adaptive"}),
                json!({"type":"enabled","budget_tokens":1024}),
                json!({"type":"disabled"}),
            ] {
                for effort in ["low", "medium", "high", "xhigh", "max"] {
                    let body = json!({"model":"test","messages":[{"role":"user","content":"hi"}],
                        "thinking":thinking,"output_config":{"effort":effort}});
                    validate_client_options("anthropic", upstream, &body).unwrap();
                    let normalized = normalize_body("anthropic", &body).unwrap();
                    assert_eq!(normalized["reasoning"]["effort"], effort);
                    assert!(normalized.get("thinking").is_none());
                    validate_for_upstream(upstream, &normalized).unwrap();
                }
            }
        }
    }

    #[test]
    fn anthropic_system_messages_become_instructions_without_losing_conversation() {
        let body = json!({"system":[{"type":"text","text":"Base instruction"}],"messages":[
            {"role":"user","content":"Question"},
            {"role":"system","content":[{"type":"text","text":"Session instruction","cache_control":{"type":"ephemeral"}}]},
            {"role":"assistant","content":"Answer"},
            {"role":"developer","content":"Developer instruction"}
        ]});
        let normalized = normalize_body("anthropic", &body).unwrap();
        assert_eq!(
            normalized["instructions"],
            "Base instruction\n\nSession instruction\n\nDeveloper instruction"
        );
        assert_eq!(normalized["input"].as_array().unwrap().len(), 2);
        assert_eq!(normalized["input"][0]["role"], "user");
        assert_eq!(normalized["input"][0]["content"][0]["text"], "Question");
        assert_eq!(normalized["input"][1]["role"], "assistant");
        assert_eq!(normalized["input"][1]["content"][0]["text"], "Answer");
        let unknown = json!({"messages":[{"role":"tool","content":"invalid"}]});
        assert!(normalize_body("anthropic", &unknown).is_err());
        let non_text =
            json!({"messages":[{"role":"system","content":[{"type":"image","source":{}}]}]});
        assert!(normalize_body("anthropic", &non_text)
            .unwrap_err()
            .contains("指令只支持文本内容"));
    }

    #[test]
    fn anthropic_thinking_without_effort_does_not_invent_removed_levels() {
        for (thinking, effort) in [
            (json!({"type":"adaptive"}), None),
            (json!({"type":"disabled"}), None),
            (json!({"type":"enabled","budget_tokens":1024}), Some("low")),
            (
                json!({"type":"enabled","budget_tokens":10000}),
                Some("medium"),
            ),
            (
                json!({"type":"enabled","budget_tokens":32000}),
                Some("high"),
            ),
        ] {
            let body = json!({"messages":[],"thinking":thinking});
            validate_client_options("anthropic", "responses", &body).unwrap();
            let normalized = normalize_body("anthropic", &body).unwrap();
            assert_eq!(
                normalized
                    .pointer("/reasoning/effort")
                    .and_then(Value::as_str),
                effort
            );
        }
    }

    #[test]
    fn malformed_thinking_and_effort_are_still_rejected() {
        for thinking in [
            json!({"type":"enabled"}),
            json!({"type":"enabled","budget_tokens":0}),
            json!({"type":"enabled","budget_tokens":"1024"}),
            json!({"type":"unknown"}),
            json!("adaptive"),
        ] {
            let body = json!({"thinking":thinking,"output_config":{"effort":"high"}});
            assert!(validate_client_options("anthropic", "responses", &body).is_err());
        }
        for effort in [json!("ultra"), json!("minimal"), json!("none"), json!(1)] {
            let body = json!({"thinking":{"type":"adaptive"},"output_config":{"effort":effort}});
            assert!(validate_client_options("anthropic", "responses", &body).is_err());
        }
        let body = json!({"reasoning":{"effort":"ultracode"}});
        assert!(validate_client_options("responses", "anthropic", &body).is_err());
    }

    #[test]
    fn converted_effort_must_belong_to_the_upstream_protocol() {
        for upstream in ["responses", "openai_chat", "anthropic"] {
            for effort in crate::domain::capabilities::effort_levels(upstream) {
                let body = json!({"input":"hi","reasoning":{"effort":effort}});
                validate_for_upstream(upstream, &body).unwrap();
            }
            for effort in [json!("none"), json!("minimal"), json!(42)] {
                assert!(validate_for_upstream(
                    upstream,
                    &json!({"input":"hi","reasoning":{"effort":effort}})
                )
                .is_err());
            }
        }
        assert!(
            validate_for_upstream("responses", &json!({"reasoning":{"effort":"ultracode"}}))
                .is_err()
        );
        assert!(validate_for_upstream(
            "anthropic",
            &json!({"input":"hi","reasoning":{"effort":"ultra"}})
        )
        .is_err());
    }

    #[test]
    fn translated_metadata_preserves_identity_and_checks_upstream_limits() {
        let metadata = json!({"user_id":"{\"session_id\":\"test-session\",\"account_uuid\":\"test-account\"}"});
        for client in ["anthropic", "openai_chat", "responses"] {
            let body = json!({"model":"test","messages":[],"input":"hi","metadata":metadata});
            let normalized = normalize_body(client, &body).unwrap();
            assert_eq!(normalized["metadata"], metadata);
            for upstream in ["responses", "openai_chat"] {
                validate_for_upstream(upstream, &normalized).unwrap();
            }
        }
        for metadata in [
            json!("user-id"),
            json!({"user_id":{}}),
            json!({"user_id":"x".repeat(513)}),
        ] {
            assert!(
                validate_for_upstream("responses", &json!({"input":"hi","metadata":metadata}))
                    .is_err()
            );
        }
    }

    #[test]
    fn chat_parallel_tool_calls_false_passes_anthropic_validation() {
        let body = normalize_body(
            "openai_chat",
            &json!({
                "model": "claude-sonnet",
                "messages": [{"role": "user", "content": "hi"}],
                "parallel_tool_calls": false,
            }),
        )
        .unwrap();
        assert!(
            validate_for_upstream("anthropic", &body).is_ok(),
            "parallel_tool_calls=false 应能通过 Anthropic 上游校验"
        );
    }

    #[test]
    fn chat_parallel_tool_calls_true_passes_anthropic_validation() {
        let body = normalize_body(
            "openai_chat",
            &json!({
                "model": "claude-sonnet",
                "messages": [{"role": "user", "content": "hi"}],
                "parallel_tool_calls": true,
            }),
        )
        .unwrap();
        assert!(validate_for_upstream("anthropic", &body).is_ok());
    }

    #[test]
    fn anthropic_disable_parallel_tool_use_normalizes_to_parallel_tool_calls() {
        let body = normalize_body(
            "anthropic",
            &json!({
                "model": "gpt-5",
                "messages": [{"role": "user", "content": "hi"}],
                "tools": [{"name": "shell", "description": "run",
                           "input_schema": {"type": "object"}}],
                "tool_choice": {"type": "auto", "disable_parallel_tool_use": true},
            }),
        )
        .unwrap();
        assert_eq!(body["tool_choice"], json!("auto"));
        assert_eq!(body["parallel_tool_calls"], json!(false));
    }

    #[test]
    fn anthropic_disable_parallel_tool_use_false_normalizes_to_parallel_tool_calls_true() {
        let body = normalize_body(
            "anthropic",
            &json!({
                "model": "gpt-5",
                "messages": [{"role": "user", "content": "hi"}],
                "tool_choice": {"type": "any", "disable_parallel_tool_use": false},
            }),
        )
        .unwrap();
        assert_eq!(body["parallel_tool_calls"], json!(true));
    }

    #[test]
    fn anthropic_disable_parallel_tool_use_must_be_boolean() {
        let error = normalize_body(
            "anthropic",
            &json!({
                "model": "gpt-5",
                "messages": [{"role": "user", "content": "hi"}],
                "tool_choice": {"type": "auto", "disable_parallel_tool_use": "yes"},
            }),
        )
        .unwrap_err();
        assert!(error.contains("disable_parallel_tool_use"));
    }

    #[test]
    fn responses_web_search_tools_pass_anthropic_validation() {
        let body = normalize_body(
            "responses",
            &json!({
                "model": "LongCat-2.5-Preview",
                "input": [{"type": "message", "role": "user",
                           "content": [{"type": "input_text", "text": "hi"}]}],
                "tools": [
                    {"type": "function", "name": "shell",
                     "parameters": {"type": "object"}},
                    {"type": "web_search"},
                    {"type": "web_search_preview"},
                ],
            }),
        )
        .unwrap();
        assert!(
            validate_for_upstream("anthropic", &body).is_ok(),
            "规范格式已建模的内置工具应通过校验，由翻译层决定省略"
        );
    }

    #[test]
    fn responses_unknown_tool_types_still_rejected_for_anthropic() {
        let body = normalize_body(
            "responses",
            &json!({
                "model": "m",
                "input": [{"type": "message", "role": "user",
                           "content": [{"type": "input_text", "text": "hi"}]}],
                "tools": [{"type": "computer_use_preview"}],
            }),
        )
        .unwrap();
        let error = validate_for_upstream("anthropic", &body).unwrap_err();
        assert!(error.contains("computer_use_preview"));
    }
}
