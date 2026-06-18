use anyhow::{Context, Result};
use base64::Engine;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

#[derive(Debug, Deserialize)]
struct YahooWireMessage {
    #[serde(rename = "type")]
    msg_type: String,
    message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct YahooPricing {
    pub symbol: Option<String>,
    pub price: Option<f32>,
    pub time: Option<u64>,
    pub exchange: Option<String>,
    pub quote_type: Option<u64>,
    pub market_hours: Option<u64>,
    pub change_percent: Option<f32>,
    pub change: Option<f32>,
    pub price_hint: Option<u64>,
}

fn read_varint(bytes: &[u8], start: usize) -> Option<(u64, usize)> {
    let mut shift = 0u32;
    let mut value = 0u64;
    let mut index = start;
    while index < bytes.len() && shift <= 63 {
        let b = bytes[index];
        value |= ((b & 0x7f) as u64) << shift;
        index += 1;
        if b & 0x80 == 0 {
            return Some((value, index));
        }
        shift += 7;
    }
    None
}

pub fn decode_pricing_message(encoded: &str) -> Result<YahooPricing> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("base64 decode yahoo message")?;
    let mut out = YahooPricing {
        symbol: None,
        price: None,
        time: None,
        exchange: None,
        quote_type: None,
        market_hours: None,
        change_percent: None,
        change: None,
        price_hint: None,
    };
    let mut i = 0usize;
    while i < bytes.len() {
        let Some((key, next)) = read_varint(&bytes, i) else { break };
        let field = key >> 3;
        let wire = key & 0x07;
        i = next;
        match (field, wire) {
            (1, 2) | (5, 2) => {
                let Some((len, next)) = read_varint(&bytes, i) else { break };
                let len = len as usize;
                i = next;
                if i + len > bytes.len() { break; }
                let slice = &bytes[i..i + len];
                if let Ok(text) = std::str::from_utf8(slice) {
                    if field == 1 {
                        out.symbol = Some(text.to_string());
                    } else {
                        out.exchange = Some(text.to_string());
                    }
                }
                i += len;
            }
            (2, 5) | (8, 5) | (12, 5) => {
                if i + 4 > bytes.len() { break; }
                let raw = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                let value = f32::from_le_bytes(raw);
                match field {
                    2 => out.price = Some(value),
                    8 => out.change_percent = Some(value),
                    12 => out.change = Some(value),
                    _ => {}
                }
                i += 4;
            }
            (3, 0) | (6, 0) | (7, 0) | (27, 0) => {
                let Some((value, next)) = read_varint(&bytes, i) else { break };
                match field {
                    3 => out.time = Some(value),
                    6 => out.quote_type = Some(value),
                    7 => out.market_hours = Some(value),
                    27 => out.price_hint = Some(value),
                    _ => {}
                }
                i = next;
            }
            (_, 0) => {
                let Some((_, next)) = read_varint(&bytes, i) else { break };
                i = next;
            }
            (_, 1) => {
                if i + 8 > bytes.len() { break; }
                i += 8;
            }
            (_, 2) => {
                let Some((len, next)) = read_varint(&bytes, i) else { break };
                let len = len as usize;
                i = next.saturating_add(len);
                if i > bytes.len() { break; }
            }
            (_, 5) => {
                if i + 4 > bytes.len() { break; }
                i += 4;
            }
            _ => break,
        }
    }
    Ok(out)
}

pub async fn subscribe_forever<F>(symbols: &[String], mut on_item: F) -> Result<()>
where
    F: FnMut(YahooPricing) + Send,
{
    let (mut ws, _) = connect_async("wss://streamer.finance.yahoo.com/?version=2")
        .await
        .context("connect yahoo websocket")?;
    let payload = json!({ "subscribe": symbols });
    ws.send(WsMessage::Text(payload.to_string())).await
        .context("send yahoo subscribe")?;

    while let Some(frame) = ws.next().await {
        let frame = frame.context("read yahoo frame")?;
        let text = match frame {
            WsMessage::Text(text) => text,
            WsMessage::Binary(bytes) => String::from_utf8(bytes.to_vec()).context("binary text frame")?,
            WsMessage::Ping(payload) => {
                ws.send(WsMessage::Pong(payload)).await.ok();
                continue;
            }
            WsMessage::Pong(_) | WsMessage::Close(_) => continue,
            _ => continue,
        };
        let wire: YahooWireMessage = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if wire.msg_type != "pricing" {
            continue;
        }
        let Some(message) = wire.message.as_deref() else { continue };
        if let Ok(decoded) = decode_pricing_message(message) {
            on_item(decoded);
        }
    }
    Ok(())
}
