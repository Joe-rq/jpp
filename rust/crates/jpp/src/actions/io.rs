//! 读写与记录：`record_check`、`read_json`、`write_json`（C-1 从 CLI 的注册闭包原样搬来）。

use super::Ctx;
use crate::interp::json_to_value;
use crate::value::Value;
use serde_json::Value as Json;

// The source computes validity. This action only records and returns its value.
pub(super) fn record_check(ctx: &Ctx, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("record_check expects one source-computed record".into());
    }
    ctx.checks.borrow_mut().push(args[0].to_json());
    Ok(args[0].clone())
}

pub(super) fn read_json(ctx: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [Value::Text(path, _)] = args else {
        return Err("read_json expects one file path".into());
    };
    let bytes = std::fs::read(找文件(ctx, path)?).map_err(|e| format!("{path}: {e}"))?;
    let value: Json = serde_json::from_slice(&bytes).map_err(|e| format!("{path}: {e}"))?;
    // Reject unsigned integers that the core's JSON adapter cannot represent as Int.
    super::validate_numbers(&value)?;
    Ok(json_to_value(&value))
}

/// `read_json` 的路径（现场稳定性三修 (2)，与 `import` 一致）：相对路径先按程序文件所在目录找，没有再按当前目录；
/// 两处都没有时报错列出两个完整路径。绝对路径、没有程序目录（库调用方）时原样用。
fn 找文件(ctx: &Ctx, path: &str) -> Result<std::path::PathBuf, String> {
    let p = std::path::Path::new(path);
    let Some(dir) = ctx.program_dir.as_ref().filter(|_| p.is_relative()) else {
        return Ok(p.to_path_buf());
    };
    let 程序旁 = dir.join(p);
    if 程序旁.exists() {
        return Ok(程序旁);
    }
    if p.exists() {
        return Ok(p.to_path_buf());
    }
    let 当前 = std::env::current_dir()
        .map(|d| d.join(p).display().to_string())
        .unwrap_or_else(|_| path.to_string());
    Err(format!(
        "{path}: 找不到文件（按程序所在目录 {}，按当前目录 {当前}）",
        程序旁.display()
    ))
}

pub(super) fn write_json(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [Value::Text(path, _), value] = args else {
        return Err("write_json expects a file path and a value".into());
    };
    let bytes = serde_json::to_vec_pretty(&value.to_json()).map_err(|e| e.to_string())?;
    std::fs::write(path.as_ref(), bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok(value.clone())
}
