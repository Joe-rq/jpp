//! 超窗裂变（`fission`，`12` §4 pass 3、`11` §5.3、L-035）的纯函数：切点规则与打分题的计数合回。
//!
//! 只依赖文本、窗口数值与调用者给的估计函数，所以放在 L0：计划（`jpp-plan::passes::fission`）与
//! 运行时（`jpp-runtime::fission`）引同一份，切点规则只有这一处（主控 2026-09-29 答复第 3 条）。
//!
//! **切点规则（递归二分，主控 Q-F3）。** 一段超窗文本：先在「两块都 ≤ W」的句界里取离字符中点最近的；
//! 没有就在同条件的词界里取；都没有时取离中点最近的句界（再没有取词界、再没有取字符中点），对仍超窗的一半
//! 递归。最后一层能一刀切成两块都 ≤ W 时，规则就是实验 V8 的 `材料.py::split`（V8 估计、W = 250 时
//! 六份材料 6/6 逐字节复现，`jpp-plan/tests/ablation/fission.rs`）。后块去掉开头的空格。
//! 句界：`。！？` 与换行之后；`.` 之后紧跟空格加大写字母、数字、`"` 或 `(`。词界：空格、`，、；：` 之后。
//! 中点按字符（Unicode 标量）数。V8 只实测过切两块；多块切分未经实测（过程记录 工程-步23b.md §六·1）。
//!
//! 依据：`11` §5.3；`12` §4 pass 3；`21` 步 23b 与 V8（`地基/过程记录/工程-步23b.md` §三、§五）

/// 切点的种类（报告与测试用）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutKind {
    /// 句界，两块都在窗内
    Sentence,
    /// 词界，两块都在窗内
    Word,
    /// 句界，还有一块超窗（要再切）
    SentenceOver,
    /// 词界，还有一块超窗
    WordOver,
    /// 没有句界与词界，按字符中点硬切
    Char,
}

impl CutKind {
    pub fn name(self) -> &'static str {
        match self {
            CutKind::Sentence => "句界",
            CutKind::Word => "词界",
            CutKind::SentenceOver => "句界*",
            CutKind::WordOver => "词界*",
            CutKind::Char => "字符*",
        }
    }
}

/// 句界位置（字符下标，切在它之前；不含 0 与末尾）
fn sentence_cuts(cs: &[char]) -> Vec<usize> {
    let mut out = vec![];
    for i in 0..cs.len().saturating_sub(1) {
        let c = cs[i];
        let after = i + 1;
        let hit = matches!(c, '。' | '！' | '？' | '\n')
            || (c == '.'
                && cs.get(i + 1) == Some(&' ')
                && cs.get(i + 2).is_some_and(|n| {
                    n.is_ascii_uppercase() || n.is_ascii_digit() || *n == '"' || *n == '('
                }));
        if hit && after < cs.len() {
            out.push(after);
        }
    }
    out
}

/// 词界位置（字符下标，切在它之前）
fn word_cuts(cs: &[char]) -> Vec<usize> {
    (0..cs.len().saturating_sub(1))
        .filter(|&i| matches!(cs[i], ' ' | '，' | '、' | '；' | '：'))
        .map(|i| i + 1)
        .collect()
}

/// 一刀：返回（前块, 后块, 种类）。文本至少两个字符。
pub fn cut_once(
    text: &str,
    window: usize,
    est: &dyn Fn(&str) -> usize,
) -> (String, String, CutKind) {
    let cs: Vec<char> = text.chars().collect();
    let at = |c: usize| -> (String, String) {
        let front: String = cs[..c].iter().collect();
        let back: String = cs[c..].iter().collect();
        (front, back.trim_start_matches(' ').to_string())
    };
    // 离字符中点的距离按两倍整数算（|2c − n|），与 V8 的 |c − n/2| 同序，不用浮点
    let n = cs.len();
    let nearest = |v: &[usize]| -> Option<usize> {
        v.iter().copied().min_by_key(|c| ((2 * c).abs_diff(n), *c))
    };
    let fits = |c: usize| {
        let (f, b) = at(c);
        est(&f) <= window && est(&b) <= window
    };
    let sc = sentence_cuts(&cs);
    let wc = word_cuts(&cs);
    let sc_ok: Vec<usize> = sc.iter().copied().filter(|c| fits(*c)).collect();
    let wc_ok: Vec<usize> = wc.iter().copied().filter(|c| fits(*c)).collect();
    for (cands, kind) in [
        (&sc_ok, CutKind::Sentence),
        (&wc_ok, CutKind::Word),
        (&sc, CutKind::SentenceOver),
        (&wc, CutKind::WordOver),
    ] {
        if let Some(c) = nearest(cands) {
            let (f, b) = at(c);
            return (f, b, kind);
        }
    }
    let c = (cs.len() / 2).max(1);
    let (f, b) = at(c);
    (f, b, CutKind::Char)
}

/// 切成块：每块 `est ≤ window`；本来就在窗内返回原文一块。切不开（一个字符还超窗）的块原样留下，
/// 调用者据 `est` 自己核（运行时照整篇判断并告警）。
pub fn split(text: &str, window: usize, est: &dyn Fn(&str) -> usize) -> Vec<String> {
    if est(text) <= window || text.chars().count() < 2 {
        return vec![text.to_string()];
    }
    let (f, b, _) = cut_once(text, window, est);
    // 防空块：切点落在开头或后块全是空格时不再往下切
    if f.is_empty() || b.is_empty() {
        return vec![text.to_string()];
    }
    let mut out = split(&f, window, est);
    out.extend(split(&b, window, est));
    out
}

/// 打分题按出口计数合回（`11` §5.3「measure → 按出口计数」）：`levels` 是各块已决的档号，`unsure` 是未决块数。
/// 票数最多的档，票数大于次多档票数加未决块数才已决（强 Kleene：未决块不论解析成哪一档，结果都不变，B51-R1）。
/// 返回 `Ok(档)` 已决；`Err(true)` 已决块之间并列（出 `unsure(tie)`）；`Err(false)` 其余未决（原因取未决块的）。
pub fn measure_count(levels: &[usize], unsure: usize) -> Result<usize, bool> {
    let mut counts: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    for l in levels {
        *counts.entry(*l).or_default() += 1;
    }
    let mut v: Vec<(usize, usize)> = counts.into_iter().collect();
    // 票数降序，同票按档号升序
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let Some(&(top, n1)) = v.first() else {
        return Err(false);
    };
    let n2 = v.get(1).map(|x| x.1).unwrap_or(0);
    if n1 > n2 + unsure {
        Ok(top)
    } else if n1 == n2 {
        Err(true)
    } else {
        Err(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v8(s: &str) -> usize {
        (s.chars().count() as f64 / 1.3) as usize + 1
    }

    #[test]
    fn 在窗内不切() {
        assert_eq!(split("短。", 10, &v8), vec!["短。".to_string()]);
    }

    #[test]
    fn 句界优先且后块去空格() {
        let s = "Aaaa aaaa. Bbbb bbbb. Cccc cccc.";
        let (f, b, k) = cut_once(s, 20, &v8);
        assert_eq!(k, CutKind::Sentence);
        assert_eq!(f, "Aaaa aaaa. Bbbb bbbb.");
        assert_eq!(b, "Cccc cccc.");
    }

    #[test]
    fn 递归到每块在窗内() {
        let s = "一二三四五。六七八九十。甲乙丙丁戊。己庚辛壬癸。";
        let blocks = split(s, 6, &v8);
        assert!(blocks.iter().all(|b| v8(b) <= 6), "{blocks:?}");
        assert_eq!(blocks.concat(), s);
    }

    #[test]
    fn 计数合回() {
        assert_eq!(measure_count(&[2, 2, 1], 0), Ok(2));
        assert_eq!(measure_count(&[2, 2, 1], 1), Err(false));
        assert_eq!(measure_count(&[2, 1], 0), Err(true));
        assert_eq!(measure_count(&[], 3), Err(false));
        assert_eq!(measure_count(&[0, 0, 0], 2), Ok(0));
    }
}
