// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
use bigbrother::{
    observation::Limits,
    runtime::{Action, NativeBackend, Runtime},
    Error, ErrorCode,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub fn observe(app: &str, depth: usize) -> anyhow::Result<()> {
    let mut runtime = Runtime::new(NativeBackend::new(app)?);
    let state = runtime.observe(
        Limits {
            max_depth: depth,
            ..Limits::default()
        },
        true,
    )?;
    println!("{}", serde_json::to_string(&state)?);
    Ok(())
}
#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}
#[derive(Deserialize)]
struct ObserveParams {
    #[serde(default)]
    limits: Limits,
    #[serde(default)]
    full: bool,
}
#[derive(Deserialize)]
struct ActParams {
    revision: u64,
    reference: String,
    #[serde(flatten)]
    action: Action,
}

pub fn serve(app: &str) -> anyhow::Result<()> {
    let mut runtime = Runtime::new(NativeBackend::new(app)?);
    let input = std::io::stdin();
    let mut reader = input.lock();
    let mut output = std::io::stdout().lock();
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        // Limit allocation before parsing; a giant unterminated request cannot grow forever.
        let n = std::io::Read::take(&mut reader, 1_048_577).read_until(b'\n', &mut bytes)?;
        if n == 0 {
            break;
        }
        if bytes.len() > 1_048_576 {
            writeln!(
                output,
                "{}",
                json!({"id":null,"error":{"code":"INVALID_REQUEST","message":"Request exceeds 1 MiB"}})
            )?;
            output.flush()?;
            break;
        }
        let request = serde_json::from_slice::<Request>(&bytes);
        let response = match request {
            Err(e) => json!({"id":null,"error":{"code":"INVALID_REQUEST","message":e.to_string()}}),
            Ok(req) => {
                let result: std::result::Result<Value, Error> = (|| match req.method.as_str() {
                    "capabilities" => Ok(
                        json!({"protocol":1,"observe":true,"diffs":true,"element_actions":["click","set_value","expand","collapse"],"actions_require_revision":true,"screenshots":"bb screenshot (separate capture command)","source":if cfg!(target_os="windows"){"windows_uia"}else{"macos_ax"}}),
                    ),
                    "observe" => {
                        let p: ObserveParams = serde_json::from_value(if req.params.is_null() {
                            json!({})
                        } else {
                            req.params
                        })
                        .map_err(|e| Error::new(ErrorCode::InvalidRequest, e.to_string()))?;
                        Ok(serde_json::to_value(runtime.observe(p.limits, p.full)?).unwrap())
                    }
                    "act" => {
                        let p: ActParams = serde_json::from_value(req.params)
                            .map_err(|e| Error::new(ErrorCode::InvalidRequest, e.to_string()))?;
                        runtime.act(p.revision, &p.reference, p.action)?;
                        Ok(json!({"acted":true,"observe_required":true}))
                    }
                    _ => Err(Error::new(
                        ErrorCode::InvalidRequest,
                        "Unknown method; use capabilities, observe or act",
                    )),
                })();
                match result {
                    Ok(result) => json!({"id":req.id,"result":result}),
                    Err(error) => json!({"id":req.id,"error":error}),
                }
            }
        };
        writeln!(output, "{response}")?;
        output.flush()?;
    }
    Ok(())
}
