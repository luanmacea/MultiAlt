// O protocolo com o processo Python (docs/features/python-automation.md,
// "Protocolo"): JSON por linha, UTF-8. O app manda o `hello` primeiro e, ao
// Parar, `{"event":"stop"}`; o Python manda pedidos `{"id","op","args"}` e
// recebe `{"id","ok":true,"result"}` ou `{"id","ok":false,"error"}`. Pedido com
// `"id": null` (log e status) não tem resposta.
//
// O mesmo `serve` atende o stdin/stdout do processo filho e o named pipe do
// Modo desenvolvedor. Cada pedido roda numa task própria: no modo "todas
// juntas" várias threads do Python esperam respostas ao mesmo tempo.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use super::errors::PyError;
use super::primitives::{dispatch, RunCtx};
use super::types::{now_ms, PythonLogLine};

/// Linha maior que isto é descartada (com aviso), sem derrubar a execução.
const MAX_LINE: usize = 1024 * 1024;
const TICK: Duration = Duration::from_millis(50);
const WATCHDOG_EVERY: Duration = Duration::from_secs(1);

pub fn system_line(run_id: &str, user_id: Option<i64>, level: &str, message: impl Into<String>) -> PythonLogLine {
    PythonLogLine {
        run_id: Some(run_id.to_string()),
        user_id,
        level: level.into(),
        message: message.into(),
        code: None,
        hint: None,
        ts: now_ms(),
    }
}

/// O erro de uma trava vai para o log em destaque (`blocked`, com a dica).
fn log_error(ctx: &RunCtx, args: &Value, error: &PyError) {
    if !error.code.is_block() {
        return;
    }
    ctx.sink.log(PythonLogLine {
        run_id: Some(ctx.run_id.clone()),
        user_id: args.get("user_id").and_then(Value::as_i64),
        level: "blocked".into(),
        message: error.message.clone(),
        code: Some(error.code.as_str().into()),
        hint: error.hint.clone(),
        ts: now_ms(),
    });
}

fn response(id: &Value, result: Result<Value, PyError>) -> String {
    match result {
        Ok(result) => json!({"id": id, "ok": true, "result": result}).to_string(),
        Err(error) => json!({"id": id, "ok": false, "error": error.to_json()}).to_string(),
    }
}

async fn write_line<W: AsyncWrite + Unpin>(writer: &mut W, line: &str) -> Result<(), String> {
    writer.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
    writer.write_all(b"\n").await.map_err(|e| e.to_string())?;
    writer.flush().await.map_err(|e| e.to_string())
}

/// Atende uma execução até o Python fechar o canal. No fim (ou se o canal cai)
/// a execução é dada como parada: o que ainda esperava a fila desiste, e o
/// lease, as teclas e os quadros são soltos.
pub async fn serve<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    ctx: RunCtx,
    hello: Value,
    reader: R,
    mut writer: W,
) -> Result<(), String> {
    let ctx = Arc::new(ctx);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let mut tasks = tokio::task::JoinSet::new();
    let mut lines = reader.lines();
    let mut stop_sent = false;
    let mut last_watchdog = Instant::now();
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut result = write_line(&mut writer, &hello.to_string()).await;
    while result.is_ok() {
        if !stop_sent && ctx.stop.load(Ordering::SeqCst) {
            stop_sent = true;
            result = write_line(&mut writer, &json!({"event": "stop"}).to_string()).await;
            continue;
        }
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => handle_line(&ctx, &tx, &mut tasks, line).await,
                    Ok(None) => break,
                    Err(e) => {
                        // Bytes que não são UTF-8 também caem aqui: avisa e segue.
                        ctx.sink.log(system_line(&ctx.run_id, None, "warn", format!("Unreadable data from Python: {e}")));
                        if e.kind() != std::io::ErrorKind::InvalidData {
                            break;
                        }
                    }
                }
            }
            Some(out) = rx.recv() => {
                // A resposta "stopped" de um pedido pode ficar pronta antes de o
                // laço voltar ao topo: o evento de Parar sai antes dela.
                if !stop_sent && ctx.stop.load(Ordering::SeqCst) {
                    stop_sent = true;
                    result = write_line(&mut writer, &json!({"event": "stop"}).to_string()).await;
                    if result.is_err() {
                        break;
                    }
                }
                result = write_line(&mut writer, &out).await;
            }
            _ = ticker.tick() => {
                if last_watchdog.elapsed() >= WATCHDOG_EVERY {
                    last_watchdog = Instant::now();
                    if let Some(expired) = ctx.focus.expire_idle(&ctx.run_id, Instant::now(), ctx.desk.as_ref()) {
                        ctx.sink.log(system_line(
                            &ctx.run_id,
                            Some(expired.user_id),
                            "system",
                            "Focus released: the script held the focus queue for 30 s without renewing it.",
                        ));
                    }
                }
            }
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
        }
    }

    // O canal fechou: quem ainda espera (fila, sleep) desiste.
    ctx.stop.store(true, Ordering::SeqCst);
    let drain = async {
        while tasks.join_next().await.is_some() {}
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), drain).await;
    tasks.abort_all();
    // Respostas que ficaram prontas ainda vão, se o canal deixar.
    while let Ok(out) = rx.try_recv() {
        if write_line(&mut writer, &out).await.is_err() {
            break;
        }
    }
    let released = ctx.finish();
    if !released.is_empty() {
        ctx.sink.log(system_line(&ctx.run_id, None, "system", format!("Released held keys: {}", released.join(", "))));
    }
    result.or(Ok(())).map(|_| ())
}

async fn handle_line(
    ctx: &Arc<RunCtx>,
    tx: &tokio::sync::mpsc::UnboundedSender<String>,
    tasks: &mut tokio::task::JoinSet<()>,
    line: String,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if trimmed.len() > MAX_LINE {
        ctx.sink.log(system_line(&ctx.run_id, None, "warn", "Ignored a line from Python longer than 1 MB."));
        return;
    }
    let message: Value = match serde_json::from_str(trimmed) {
        Ok(v @ Value::Object(_)) => v,
        _ => {
            let shown: String = trimmed.chars().take(300).collect();
            ctx.sink.log(system_line(
                &ctx.run_id,
                None,
                "warn",
                format!("Ignored a line from Python that is not a protocol message: {shown}"),
            ));
            return;
        }
    };
    let Some(op) = message.get("op").and_then(Value::as_str).map(str::to_string) else {
        ctx.sink.log(system_line(&ctx.run_id, None, "warn", "Ignored a message from Python without \"op\"."));
        return;
    };
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let args = message.get("args").cloned().unwrap_or(Value::Null);
    if id.is_null() {
        // Fire-and-forget: log e status, sem resposta.
        if let Err(error) = dispatch(ctx, &op, args.clone()).await {
            log_error(ctx, &args, &error);
        }
        return;
    }
    let ctx = ctx.clone();
    let tx = tx.clone();
    tasks.spawn(async move {
        let result = dispatch(&ctx, &op, args.clone()).await;
        if let Err(error) = &result {
            log_error(&ctx, &args, error);
        }
        let _ = tx.send(response(&id, result));
    });
}

#[cfg(test)]
mod python_bridge_tests {
    use super::*;
    use crate::python::primitives::python_primitives_tests::{account, own_queue, FakeDesk, NullSink};
    use std::sync::atomic::AtomicBool;
    use tokio::io::{AsyncReadExt, BufReader};

    struct Harness {
        to_app: tokio::io::DuplexStream,
        from_app: tokio::io::Lines<BufReader<tokio::io::DuplexStream>>,
        sink: Arc<NullSink>,
        stop: Arc<AtomicBool>,
        task: tokio::task::JoinHandle<Result<(), String>>,
    }

    fn start(stop_now: bool) -> Harness {
        let (to_app, app_reads) = tokio::io::duplex(64 * 1024);
        let (app_writes, from_app) = tokio::io::duplex(64 * 1024);
        let sink = Arc::new(NullSink::default());
        let stop = Arc::new(AtomicBool::new(stop_now));
        let desk = FakeDesk::with_window(1, 11);
        let ctx = RunCtx::new("r1", vec![account(1, "Main_01")], stop.clone(), sink.clone(), desk, own_queue());
        let hello = json!({"event": "hello", "run": {"id": "r1"}, "accounts": []});
        let task = tokio::spawn(serve(ctx, hello, BufReader::new(app_reads), app_writes));
        Harness {
            to_app,
            from_app: BufReader::new(from_app).lines(),
            sink,
            stop,
            task,
        }
    }

    impl Harness {
        async fn send(&mut self, line: &str) {
            self.to_app.write_all(format!("{line}\n").as_bytes()).await.unwrap();
        }
        async fn next(&mut self) -> Value {
            let line = tokio::time::timeout(Duration::from_secs(5), self.from_app.next_line())
                .await
                .expect("a line from the app")
                .unwrap()
                .expect("open");
            serde_json::from_str(&line).unwrap()
        }
    }

    #[tokio::test]
    async fn response_matches_request_id() {
        let mut h = start(false);
        assert_eq!(h.next().await["event"], json!("hello"));
        // O sleep longo responde depois do rápido: cada um com o seu id.
        h.send(r#"{"id":1,"op":"run.sleep","args":{"ms":150}}"#).await;
        h.send(r#"{"id":2,"op":"run.stopped","args":{}}"#).await;
        h.send(r#"{"id":3,"op":"account.cookie","args":{}}"#).await;
        let a = h.next().await;
        let b = h.next().await;
        let c = h.next().await;
        let mut by_id: Vec<Value> = vec![a, b, c];
        by_id.sort_by_key(|v| v["id"].as_i64());
        assert_eq!(by_id[0], json!({"id": 1, "ok": true, "result": true}));
        assert_eq!(by_id[1], json!({"id": 2, "ok": true, "result": false}));
        assert_eq!(by_id[2]["ok"], json!(false));
        assert_eq!(by_id[2]["error"]["code"], json!("forbidden"));
        drop(h.to_app);
        h.task.await.unwrap().unwrap();
        // O forbidden é uma trava: aparece em destaque no log.
        let lines = h.sink.lines.lock().unwrap();
        assert!(lines.iter().any(|l| l.level == "blocked" && l.code.as_deref() == Some("forbidden")));
    }

    #[tokio::test]
    async fn malformed_line_is_logged_not_fatal() {
        let mut h = start(false);
        h.next().await;
        h.send("this is not json").await;
        h.send("[1,2,3]").await;
        h.send(r#"{"id":5}"#).await;
        h.send(r#"{"id":6,"op":"run.stopped"}"#).await;
        assert_eq!(h.next().await, json!({"id": 6, "ok": true, "result": false}));
        drop(h.to_app);
        h.task.await.unwrap().unwrap();
        let warns = h.sink.lines.lock().unwrap().iter().filter(|l| l.level == "warn").count();
        assert_eq!(warns, 3);
    }

    #[tokio::test]
    async fn fire_and_forget_log_has_no_response() {
        let mut h = start(false);
        h.next().await;
        h.send(r#"{"id":null,"op":"run.log","args":{"level":"warn","message":"hello there","user_id":1}}"#).await;
        h.send(r#"{"op":"run.status","args":{"text":"Farming"}}"#).await;
        h.send(r#"{"id":9,"op":"run.stopped","args":{}}"#).await;
        // A primeira (e única) resposta é a do id 9.
        assert_eq!(h.next().await["id"], json!(9));
        drop(h.to_app);
        h.task.await.unwrap().unwrap();
        let mut rest = String::new();
        let _ = h.from_app.into_inner().read_to_string(&mut rest).await;
        assert!(rest.trim().is_empty(), "no response to fire-and-forget: {rest}");
        let lines = h.sink.lines.lock().unwrap();
        let log = lines.iter().find(|l| l.message == "hello there").expect("logged");
        assert_eq!(log.level, "warn");
        assert_eq!(log.user_id, Some(1));
        assert_eq!(log.run_id.as_deref(), Some("r1"));
    }

    #[tokio::test]
    async fn stop_event_is_sent_first() {
        let mut h = start(true);
        assert_eq!(h.next().await["event"], json!("hello"));
        // Antes de qualquer resposta, o Python fica sabendo do Parar.
        h.send(r#"{"id":1,"op":"run.sleep","args":{"ms":5000}}"#).await;
        assert_eq!(h.next().await, json!({"event": "stop"}));
        let reply = h.next().await;
        assert_eq!(reply["error"]["code"], json!("stopped"));
        // Parar no meio também manda o evento (uma vez só).
        let mut h2 = start(false);
        h2.next().await;
        h2.send(r#"{"id":1,"op":"run.sleep","args":{"ms":60000}}"#).await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        h2.stop.store(true, Ordering::SeqCst);
        assert_eq!(h2.next().await, json!({"event": "stop"}));
        assert_eq!(h2.next().await["error"]["code"], json!("stopped"));
        drop(h.to_app);
        drop(h2.to_app);
        h.task.await.unwrap().unwrap();
        h2.task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn closing_the_channel_releases_the_lease() {
        let (to_app, app_reads) = tokio::io::duplex(4096);
        let (app_writes, _from_app) = tokio::io::duplex(64 * 1024);
        let queue = own_queue();
        let ctx = RunCtx::new(
            "r1",
            vec![account(1, "Main_01")],
            Arc::new(AtomicBool::new(false)),
            Arc::new(NullSink::default()),
            FakeDesk::with_window(1, 11),
            queue.clone(),
        );
        let task = tokio::spawn(serve(ctx, json!({"event": "hello"}), BufReader::new(app_reads), app_writes));
        let mut to_app = to_app;
        to_app
            .write_all(b"{\"id\":1,\"op\":\"focus.acquire\",\"args\":{\"user_id\":1}}\n")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(queue.holds("r1", 1));
        drop(to_app); // o processo morreu
        task.await.unwrap().unwrap();
        assert!(!queue.holds("r1", 1));
    }
}
