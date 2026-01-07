use brain_core::cleaner::{apply_replace_active_cleaner, sanitize_params};
use brain_core::gemini::{generate_tone, ToneRequest};
use brain_core::protocol::{ClientCommand, MergeMode, ServerMessage};
use clap::Parser;
use std::net::TcpStream;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

#[derive(Parser, Debug)]
#[command(name = "brain_cli")]
struct Args {
    #[arg(long)]
    prompt: String,

    #[arg(long)]
    target_guid: Option<String>,

    #[arg(long, default_value = "gemini-1.5-pro")]
    gemini_model: String,

    #[arg(long, default_value = "ws://127.0.0.1:9001")]
    ws_url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| anyhow::anyhow!("missing GEMINI_API_KEY env var"))?;

    let (mut ws, _resp) = connect(args.ws_url.as_str())?;

    let (session_token, instances) = wait_handshake(&mut ws)?;
    eprintln!("handshake ok: {} instance(s)", instances.len());

    let target = if let Some(g) = args.target_guid.clone() {
        g
    } else {
        instances
            .iter()
            .find(|i| matches!(i.confidence, brain_core::protocol::Confidence::High))
            .or_else(|| instances.first())
            .ok_or_else(|| anyhow::anyhow!("no instances found"))?
            .fx_guid
            .clone()
    };

    let tone = generate_tone(
        &api_key,
        &args.gemini_model,
        ToneRequest {
            user_prompt: args.prompt.clone(),
        },
    )
    .await?;

    let mut params = sanitize_params(tone.params).map_err(|e| anyhow::anyhow!(e))?;
    params = apply_replace_active_cleaner(MergeMode::ReplaceActive, params);

    let cmd = ClientCommand::SetTone {
        session_token,
        command_id: format!("cli-{}", chrono_nanos()),
        target_fx_guid: target,
        mode: MergeMode::ReplaceActive,
        params,
    };

    ws.send(Message::Text(serde_json::to_string(&cmd)?))?;
    wait_ack(&mut ws)?;

    Ok(())
}

fn wait_handshake(
    ws: &mut WebSocket<MaybeTlsStream<TcpStream>>,
) -> anyhow::Result<(String, Vec<brain_core::protocol::GojiraInstance>)> {
    loop {
        let msg = ws.read()?;
        let Message::Text(text) = msg else { continue };
        let server: ServerMessage = serde_json::from_str(&text)?;
        if let ServerMessage::Handshake {
            session_token,
            instances,
            ..
        } = server
        {
            return Ok((session_token, instances));
        }
    }
}

fn wait_ack(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> anyhow::Result<()> {
    loop {
        let msg = ws.read()?;
        let Message::Text(text) = msg else { continue };
        let server: ServerMessage = serde_json::from_str(&text)?;
        match server {
            ServerMessage::Ack { command_id } => {
                eprintln!("ack: {command_id}");
                return Ok(());
            }
            ServerMessage::Error { msg, code } => {
                return Err(anyhow::anyhow!("server error {code:?}: {msg}"));
            }
            _ => {}
        }
    }
}

fn chrono_nanos() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
