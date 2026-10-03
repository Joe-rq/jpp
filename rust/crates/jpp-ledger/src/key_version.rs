//! 键法版本（B0630，预注册 `规划/B0630-键与格式稳定-预注册.md` §2.2–2.3）：账本头 `compared.key_version` 记这一本账
//! 的键是怎么算的，新二进制读旧账本按它选键法。
//!
//! - 缺省 = [`KeyVersion::Offset`]（旧键法，站点用源码字节偏移；B0630 之前写的账本都没有这个字段）；
//! - `"1"` = [`KeyVersion::Structured`]（站点用「定义路径 + 定义内序号」）；
//! - 其余的值不认识，报 `E-key-version`，不猜。
//!
//! 本模块只管「头里的值 ↔ 键法」与「这个场合能不能用」两件事，纯函数；真正按键法算键在 `jpp-runtime`。
//! [`KEY_VERSION_CURRENT`] 是本二进制新写的账本用的键法；[`KeyVersion::supported`] 是本二进制能算的键法。

/// 键法。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyVersion {
    /// 旧键法：站点是源码字节偏移。头里不写 `key_version`。
    Offset,
    /// 结构化键：站点是「定义路径 + 定义内序号」。头里写 `"1"`。
    Structured,
}

/// 本二进制新写的账本用的键法（只在这一处定）。
pub const KEY_VERSION_CURRENT: KeyVersion = KeyVersion::Structured;

/// 本二进制能算的键法：旧账本只凭账本重放要用得上。
pub const KEY_VERSIONS_SUPPORTED: [KeyVersion; 2] = [KeyVersion::Offset, KeyVersion::Structured];

impl KeyVersion {
    /// 头里的写法：旧键法不写（为空不写，旧账本与金样逐字节不变）。
    pub fn header_tag(self) -> Option<&'static str> {
        match self {
            KeyVersion::Offset => None,
            KeyVersion::Structured => Some("1"),
        }
    }

    /// 给人看的名字（报文用）。
    pub fn name(self) -> &'static str {
        match self {
            KeyVersion::Offset => "字节偏移（key_version 缺省）",
            KeyVersion::Structured => "结构化站点（key_version \"1\"）",
        }
    }

    /// 读头里的 `key_version`：缺省是旧键法，`"1"` 是结构化键，其余报 `E-key-version`。
    pub fn from_header(tag: Option<&str>) -> Result<KeyVersion, String> {
        match tag {
            None => Ok(KeyVersion::Offset),
            Some("1") => Ok(KeyVersion::Structured),
            Some(other) => Err(format!(
                "E-key-version: 账本头的 key_version 是 {other:?}，本二进制不认识（认识缺省与 \"1\"）。\
                 修法：用写这本账的二进制读，或不带账本重跑"
            )),
        }
    }

    /// 本二进制能不能按这个键法算键。
    pub fn supported(self) -> bool {
        KEY_VERSIONS_SUPPORTED.contains(&self)
    }
}

/// 一次运行该用哪种键法（预注册 §2.3 的表）。`ledger_tag` 是读入账本头的 `key_version`（没有账本、
/// 账本没有条目时 `ledger_has_entries` 置 `false`，按新跑处理）；`audit` 是只凭账本重放。
///
/// - 新跑（没有账本或账本为空）：本二进制的当前键法；
/// - 头里的值不认识：`E-key-version`（任何场合）；
/// - 头里的键法与当前相同：照用；
/// - 不同：审计重放按头里的键法算（对原账本逐键忠实，写回的头也写它，能再次重放），前提是本二进制能算它；
///   续接拒绝——续接会按当前键法写新键，一本账里混两种键。
pub fn choose(
    ledger_tag: Option<&str>,
    ledger_has_entries: bool,
    audit: bool,
) -> Result<KeyVersion, String> {
    // 不认识的值先拒，哪怕账本是空的：头里写了不认识的东西，说明不是本二进制写的
    let in_ledger = KeyVersion::from_header(ledger_tag)?;
    if !ledger_has_entries {
        return Ok(KEY_VERSION_CURRENT);
    }
    if in_ledger == KEY_VERSION_CURRENT {
        return Ok(in_ledger);
    }
    if audit {
        if in_ledger.supported() {
            return Ok(in_ledger);
        }
        return Err(format!(
            "E-key-version: 账本的键法是{}，本二进制不能按它算键。修法：用写这本账的二进制重放",
            in_ledger.name()
        ));
    }
    match in_ledger {
        KeyVersion::Offset => Err(format!(
            "E-key-version: 账本的键法是{}，本二进制写{}的键；续接会在一本账里混两种键。\
             修法：用 --replay 只凭账本重放；或不带账本重跑、加 --cache <旧账本所在目录>——\
             判断与生成的缓存键本来不含站点，零花费命中",
            in_ledger.name(),
            KEY_VERSION_CURRENT.name()
        )),
        KeyVersion::Structured => Err(format!(
            "E-key-version: 账本的键法是{}，本二进制写{}的键；续接会在一本账里混两种键。\
             修法：用写这本账的二进制续接，或不带账本重跑",
            in_ledger.name(),
            KEY_VERSION_CURRENT.name()
        )),
    }
}
