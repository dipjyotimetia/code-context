use std::env;

use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap};
use serde_json::Value;

fn parse_sse_messages(body: &str) -> Vec<Value> {
    let mut messages = Vec::new();
    for line in body.lines() {
        if !line.starts_with("data:") {
            continue;
        }
        let data = line[5..].trim();
        if data.is_empty() {
            continue;
        }
        if !data.starts_with('{') {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(data) {
            messages.push(value);
        }
    }
    messages
}

async fn post_sse(
    client: &reqwest::Client,
    url: &str,
    payload: &Value,
    session_id: Option<&str>,
) -> Result<(HeaderMap, Vec<Value>), Box<dyn std::error::Error>> {
    let mut req = client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .json(payload);
    if let Some(id) = session_id {
        req = req.header("Mcp-Session-Id", id);
    }
    let resp = req.send().await?;
    let headers = resp.headers().clone();
    let body = resp.text().await?;
    Ok((headers, parse_sse_messages(&body)))
}

fn require(condition: bool, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !condition {
        return Err(message.into());
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut url = "http://127.0.0.1:3001/mcp".to_string();
    let mut repo = env::current_dir()?.to_string_lossy().to_string();

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--url" => {
                if let Some(value) = args.next() {
                    url = value;
                }
            }
            "--repo" => {
                if let Some(value) = args.next() {
                    repo = value;
                }
            }
            "--help" | "-h" => {
                println!("Usage: mcp_client [--url <url>] [--repo <path>]");
                return Ok(());
            }
            _ => {}
        }
    }

    let client = reqwest::Client::new();

    let init_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "mcp-client", "version": "0.1"}
        }
    });

    let (init_headers, init_messages) = post_sse(&client, &url, &init_payload, None).await?;
    let session_id = init_headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or("initialize did not return Mcp-Session-Id")?;

    require(
        init_messages.len() == 1 && init_messages[0].get("result").is_some(),
        "initialize response missing result",
    )?;

    let init_notify = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = post_sse(&client, &url, &init_notify, Some(&session_id)).await?;

    let tools_list = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    let (_, tools_messages) = post_sse(&client, &url, &tools_list, Some(&session_id)).await?;
    require(
        tools_messages.len() == 1 && tools_messages[0].get("result").is_some(),
        "tools/list missing result",
    )?;
    let tools = tools_messages[0]["result"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    require(
        tools
            .iter()
            .any(|t| t.get("name") == Some(&Value::String("index_repository".into()))),
        "index_repository missing from tools/list",
    )?;

    let prompts_list = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "prompts/list",
        "params": {}
    });
    let (_, prompts_messages) = post_sse(&client, &url, &prompts_list, Some(&session_id)).await?;
    require(
        prompts_messages.len() == 1 && prompts_messages[0].get("result").is_some(),
        "prompts/list missing result",
    )?;

    let get_prompt = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "prompts/get",
        "params": {"name": "onboard_repository", "arguments": {"path": repo}}
    });
    let (_, get_prompt_messages) = post_sse(&client, &url, &get_prompt, Some(&session_id)).await?;
    require(
        get_prompt_messages.len() == 1 && get_prompt_messages[0].get("result").is_some(),
        "prompts/get missing result",
    )?;

    let call_tool = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {"name": "get_project_overview", "arguments": {}}
    });
    let (_, call_tool_messages) = post_sse(&client, &url, &call_tool, Some(&session_id)).await?;
    require(
        call_tool_messages.len() == 1 && call_tool_messages[0].get("result").is_some(),
        "tools/call missing result",
    )?;

    println!("MCP client verification OK");
    Ok(())
}
