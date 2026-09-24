//! 上岗的正门：`commission` 系列认证与选线辅助。（拆 calib.rs：原第 801–1386、1697 行起）

use jpp_value::value::Op;

use super::*;
use jpp_value::stat::Certificate;

impl CalibStore {
    /// **上岗的正门**：拿这条键积累来的标注样本跑保形认证，**认过才上岗，线由证书定**。
    ///
    /// `cluster_unit` **必须由调用方声明**，不许从数据推断。推断出来的默认会造出一张
    /// 写着「按条核过」的证书，**而真相是没人说过簇是什么**——那正是「给没有类型的东西
    /// 补来源」那个形状。声明「对象段」而样本没有簇 id 是**错，不是降级**。
    ///
    /// 认证不过时返回 `Certificate::Refused`，**带着 `n_needed`**——
    /// 它是唯一告诉作者「这条路有终点」的东西。**记录不动，留在 `待真值`**：
    /// 不阻塞、但也不放行。
    pub fn commission(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        cluster_unit: &str,
    ) -> Result<Cert, Refusal> {
        self.commission_inner(key, alpha, conf_delta, cluster_unit, None)
    }

    /// **代价矩阵定线、证书定能不能上岗**（那条裁定的两半合起来）。
    ///
    /// `certify` 自己会去找一条最宽的、仍被认证住的线；**给了代价矩阵就不找了**——
    /// 线由 `cost_line` 在标注集上按 `fp·#误放行 + fn·#漏放行` 最小定出来，
    /// 证书只回答**这条线在这批数据上的假放行上界够不够 α**。
    /// 这是那条裁定的字面实现：**代价决定线定在哪，证书决定能不能上岗，不是二选一。**
    pub fn commission_costed(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        cluster_unit: &str,
        cost: (f64, f64),
    ) -> Result<Cert, Refusal> {
        self.commission_inner(key, alpha, conf_delta, cluster_unit, Some(cost))
    }

    fn commission_inner(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        cluster_unit: &str,
        cost: Option<(f64, f64)>,
    ) -> Result<Cert, Refusal> {
        // **先卡区间再造证书**：α=0 会让 `n_needed_zero_error` 得到 inf，
        // 而 `serde_json::to_value` 在 NaN/inf 上失败 → `calib_hash` 把整条记录哈成 Null，
        // **两条坏记录于是哈出同一个值**。区间在进哈希之前卡住。
        let bad = |why: &str| Refusal::跑不成(why.to_string());
        if !(alpha > 0.0 && alpha < 1.0) || !(conf_delta > 0.0 && conf_delta < 1.0) {
            return Err(bad("alpha 与 conf_delta 必须在开区间 (0, 1)"));
        }
        let rec = match self.records.get(key) {
            Some(r) => r,
            None => return Err(bad("没有这条记录")),
        };
        if rec.status == "停岗" {
            // 停岗是人下的判断，不是样本数的函数——**重新认证不是复岗的通道**
            return Err(bad("停岗的键不经由 commission 复岗"));
        }
        let rec_lsrc = rec.label_source.clone();
        let 带标注: Vec<&Sample> = rec
            .samples
            .iter()
            .filter(|s| s.label.is_some() && s.p.is_some())
            .collect();
        // **一条记录只能有一个题型。** 代价路径一直查这个，而这条非代价路径从来没查过——
        // **不同尺不可比**是本项目自己的判据，认证路径上漏了一处。
        //
        // 这一条也是 `LabelSource::选择子集.与对错相关` **能是一个标量的前提**：
        // 相关性按题型分叉（noul +0.527 / choice +0.418 / **score −0.062，反向**），
        // 一条记录一个值之所以够用，**正是因为一条记录就是一个题型**。
        // 在这之前那是约定不是保证——`absorb` 收任何 `phys` 字符串。
        {
            let mut 见到: std::collections::BTreeSet<&str> = Default::default();
            for s in &带标注 {
                见到.insert(s.phys.as_str());
            }
            if 见到.len() > 1 {
                return Err(Refusal::跑不成(format!(
                    "键 {key} 的标注样本混了 {} 种物理形式（{}）：不同尺不可比，一条记录只能认一个题型的线",
                    见到.len(),
                    见到.into_iter().collect::<Vec<_>>().join(" / ")
                )));
            }
        }
        if 带标注.is_empty() {
            return Err(bad("没有带标注的样本：线只从标注记录来"));
        }
        // **给 `unsure_rate` 算出料用的两样**，趁 `rec` / `带标注` 还活着先取下来。
        // δ 的口径必须与 `cut` 里那一处**同源**（`delta_for`：记录自带的优先，否则档案的），
        // 否则「认证时测的 unsure 率」与「运行期真的会 unsure 的率」测的不是同一件事。
        let 题型认证时 = 反查题型(&带标注[0].phys);
        let 众数认证时: Vec<Option<f64>> = 带标注.iter().map(|s| s.mode_share).collect();
        // 三种题型的出口都带 δ 迟滞（B63 起 choice / score 也是），率都绑 δ
        let 绑delta = |d: Option<f64>| d;
        let delta认证时: Option<f64> = 题型认证时.map(|op| {
            rec.delta.unwrap_or(match op {
                Op::Test => self.profile.delta.0,
                Op::Select => self.profile.delta.1,
                Op::Measure => self.profile.delta.2,
            })
        });
        // **指纹算的是「用到的那些 `(p, label)` 对的规范形」，不是源文件字节。**
        // 源文件会被重新导出、重新排序——按字节算会在数据没变时乱跳，
        // **而乱跳的检查会教会人绕过它**。
        let label_fp = 标注集指纹(&带标注);
        if !rec.label_fp.is_empty() && rec.label_fp != label_fp {
            return Err(Refusal::跑不成(format!(
                "键 {key} 声明的标注集是 {:?}（指纹 {}），而这次用到的那批指纹是 {label_fp}：对不上就不认证",
                rec.label_set_id, rec.label_fp
            )));
        }
        let 按条: Vec<(f64, bool)> = 带标注
            .iter()
            .map(|s| (s.p.expect("已滤"), s.label == Some(1)))
            .collect();

        // **代价线分支**：线不由证书自己找，由代价矩阵在标注集上定。
        if let Some((fp, fn_)) = cost {
            // Python `runtime.py:1147` 直接 raise：select / measure 的代价线**未定**。
            // 让它悄悄什么也不做，比报错糟。
            if 带标注.iter().any(|s| s.phys != "noul") {
                return Err(Refusal::跑不成(format!(
                    "cut(cost=) 只对 test 题有定义（select / measure 的代价线未定）；键 {key} 上有非 noul 的样本"
                )));
            }
            // J-16：代价线与保形线不能同源。**这一条 Python 也拦**（`runtime.py:1151`）。
            let (lsid, sid) = (rec.label_set_id.clone(), rec.set_id.clone());
            if lsid.is_empty() || lsid == sid {
                return Err(Refusal::跑不成(format!(
                    "J-16: 校准键 {key} 的标注集 id（{lsid:?}）必须给出且 ≠ 保形集 id（{sid:?}）；代价线与保形线不能同源"
                )));
            }
            let cl = match jpp_value::stat::cost_line(&按条, fp, fn_) {
                Ok(c) => c,
                Err(e) => return Err(Refusal::跑不成(e)),
            };
            // 证书只回答「这条线够不够 α」，不再自己找线
            let ucb = jpp_value::stat::binomial_upper(cl.n_false_accept, cl.n_accepted, conf_delta);
            if cl.n_accepted == 0 || ucb > alpha {
                return Err(Refusal::认证不过(Certificate::Refused {
                    best_ucb: ucb,
                    best_hi: cl.line,
                    best_n_accepted: cl.n_accepted,
                    n_needed: jpp_value::stat::n_needed_zero_error(alpha, conf_delta),
                }));
            }
            let cert = Cert {
                alpha,
                conf_delta,
                hi: cl.line,
                n_accepted: cl.n_accepted,
                n_errors: cl.n_false_accept,
                ucb,
                cluster_unit: cluster_unit.into(),
                resample: None,
                cost,
                bounded_side: 单侧声明(),
                label_source: rec_lsrc.clone(),
                label_fp: label_fp.clone(),
                selection: None,
                grade: CertGrade::Formal,
            };
            let r = self.records.get_mut(key).expect("刚读过");
            r.certs.insert(cert.addr(), cert.clone());
            let 选中 = r.选中的证书().cloned().expect("刚插进去");
            r.hi = 选中.hi;
            r.lo = 0.0;
            r.status = "上岗".into();
            r.fixture = false;
            r.unsure_rate =
                经验unsure率(&按条, &众数认证时, 题型认证时, r.hi, r.lo, delta认证时);
            r.unsure_rate_delta = 绑delta(delta认证时);
            return Ok(cert);
        }
        let (cert_result, resample) = if cluster_unit == "条" {
            (jpp_value::stat::certify(&按条, alpha, conf_delta), None)
        } else {
            // 声明了簇，就必须每条都带簇 id
            if 带标注.iter().any(|s| s.cluster.is_none()) {
                return Err(bad("声明了簇单位，但有样本没有簇 id：这是错，不是降级"));
            }
            let 三元: Vec<(f64, bool, String)> = 带标注
                .iter()
                .map(|s| {
                    (
                        s.p.expect("已滤"),
                        s.label == Some(1),
                        s.cluster.clone().expect("已核"),
                    )
                })
                .collect();
            // **全过才算过**（说不准往拒绝那边倒）。这条规则**未标定**——原型只报了
            // 「200 次里有解几次」，没有定「几次算过」。记进证书是为了它可审。
            const R: usize = 200;
            let mut 最差: Option<Certificate> = None;
            let mut 全过 = true;
            let mut 成的: Option<Certificate> = None;
            for seed in 0..R as u64 {
                let c = jpp_value::stat::certify(
                    &jpp_value::stat::cluster_subsample(&三元, seed),
                    alpha,
                    conf_delta,
                );
                if c.is_refused() {
                    全过 = false;
                    最差 = Some(c);
                    break;
                }
                if 成的.is_none() {
                    成的 = Some(c);
                }
            }
            let out = if 全过 {
                成的.expect("R > 0")
            } else {
                最差.expect("刚设的")
            };
            (out, Some((R, "全过才算过（未标定）".to_string())))
        };

        match cert_result {
            Certificate::Refused { .. } => Err(Refusal::认证不过(cert_result)),
            Certificate::Line {
                hi,
                n_accepted,
                n_errors,
                ucb,
            } => {
                let cert = Cert {
                    alpha,
                    conf_delta,
                    hi,
                    n_accepted,
                    n_errors,
                    ucb,
                    cluster_unit: cluster_unit.into(),
                    resample,
                    cost,
                    bounded_side: 单侧声明(),
                    label_source: rec_lsrc.clone(),
                    label_fp: label_fp.clone(),
                    selection: None,
                    grade: CertGrade::Formal,
                };
                let r = self.records.get_mut(key).expect("刚读过");
                // 同一个地址是**更新那一格**；不同地址是**新增一格**
                r.certs.insert(cert.addr(), cert.clone());
                // `hi`/`lo` 是**选择规则算出来的视图**，不是第二处真相——
                // 它和 `line_source` 报的那张必须出自同一个函数，否则两处会各说各的。
                let 选中 = r.选中的证书().cloned().expect("刚插进去");
                r.hi = 选中.hi;
                // **线由证书定，不由调用方写。**
                //
                // `lo = 0.0`：**证书只管放行那一侧**（损失 = 放行区里的假放行），
                // 弃权那一侧它一个字也没说。没有凭据就不放行任何 `Ignore`，
                // 于是带宽最大、`Unsure` 最多——**说不准往拒绝那边倒**。
                // 保留记录原有的 `lo` 是不行的：`hi` 可能落到它下面（实测 hi=0.295 而
                // 缺省 lo=0.35），那样带就翻了，而 `put` 的 `lo ≤ hi` 也会被自己人违反。
                r.lo = 0.0;
                r.status = "上岗".into();
                r.fixture = false;
                // **J-10 的 uᵢ 在这里被测出来**（`12`:591「有标注集时用**经验**联合 unsure 率」；
                // `12`:814 那一栏写的是「✓ 估计 | **实测**」）。在这之前这个字段**只有消费方没有生产者**：
                // 走完 `absorb` → `commission` 的真实路径拿到的仍是 `None`，
                // `unsure_bound` 按 1 计，**界退化成 `union_bound == n`——一条真的、但什么也没说的界。**
                r.unsure_rate =
                    经验unsure率(&按条, &众数认证时, 题型认证时, r.hi, r.lo, delta认证时);
                r.unsure_rate_delta = 绑delta(delta认证时);
                Ok(cert)
            }
        }
    }

    /// **这个键的无标签漂移报告**（`12`:396「漂移监控（无标签：读数分布偏移 + 保形覆盖跌落告警）」）。
    ///
    /// **参照分布 = 带标注的那些**（线是在它们上面定的），
    /// **近期分布 = 运行期积累的无标注观察**（`absorb` 从 `Outcome.evidence` 收来的）。
    /// **数据已经在记录里，不需要新的数据源。**
    ///
    /// **一侧为空返回 `None`，不是「漂移为 0」**——与 `binomial_upper` 的 `n == 0 → 1.0`
    /// 同一条：**算不出来不是一个值**。
    /// **两侧联合认证**（真值通道用）。`commission` 只界定 `p ≥ hi` 一侧，并取**最宽**的线、
    /// 把 `lo` 置 0：Ignore 不可达，而且最宽的上侧线把错误预算用在边界上，给下侧留不出位置。
    ///
    /// 这里在同一批标注上联合选 `(lo, hi)`：按 `cut` 的实际判区（`p ≥ hi + δ` 给 Act、
    /// `p ≤ lo − δ` 给 Ignore）计，要求
    /// - Act 区的假放行（真值为假）二项上界 ≤ α，
    /// - Ignore 区的漏放行（真值为真）二项上界 ≤ α，
    /// - `lo ≤ hi`，两区都非空；
    ///
    /// 在满足者中取**已决条数最多**的一对。上侧证书进 `certs`（`resample` 注明选线规则），
    /// 下侧证书进 `lower`。只做「按条」。认不出就 `Refusal`，记录不动。
    pub fn commission_two_sided(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
    ) -> Result<Cert, Refusal> {
        let bad = |why: &str| Refusal::跑不成(why.to_string());
        if !(alpha > 0.0 && alpha < 1.0) || !(conf_delta > 0.0 && conf_delta < 1.0) {
            return Err(bad("alpha 与 conf_delta 必须在开区间 (0, 1)"));
        }
        let rec = self.records.get(key).ok_or_else(|| bad("没有这条记录"))?;
        if rec.status == "停岗" {
            return Err(bad("停岗的键不经由认证复岗"));
        }
        let 带标注: Vec<&Sample> = rec
            .samples
            .iter()
            .filter(|s| s.label.is_some() && s.p.is_some())
            .collect();
        if 带标注.is_empty() {
            return Err(bad("没有带标注的样本：线只从标注记录来"));
        }
        if 带标注.iter().any(|s| s.phys != "noul") {
            return Err(bad("两侧认证只对 test 题有定义"));
        }
        let delta = rec.delta.unwrap_or(self.profile.delta.0);
        let 按条: Vec<(f64, bool)> = 带标注
            .iter()
            .map(|s| (s.p.expect("已滤"), s.label == Some(1)))
            .collect();
        let label_fp = 标注集指纹(&带标注);
        let lsrc = rec.label_source.clone();
        // 候选判区边界：每个读数本身与相邻读数的中点
        let mut ps: Vec<f64> = 按条.iter().map(|x| x.0).collect();
        ps.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ps.dedup();
        let mut cands: Vec<f64> = ps.clone();
        for w in ps.windows(2) {
            cands.push((w[0] + w[1]) / 2.0);
        }
        cands.sort_by(|a, b| a.partial_cmp(b).unwrap());
        cands.dedup();
        // 上侧：判区 p ≥ h
        let up: Vec<(f64, usize, usize, f64)> = cands
            .iter()
            .filter_map(|&h| {
                let acc: Vec<&(f64, bool)> = 按条.iter().filter(|x| x.0 >= h).collect();
                if acc.is_empty() || h < delta {
                    return None;
                }
                let k = acc.iter().filter(|x| !x.1).count();
                let u = jpp_value::stat::binomial_upper(k, acc.len(), conf_delta);
                (u <= alpha).then_some((h, acc.len(), k, u))
            })
            .collect();
        // 下侧：判区 p ≤ l
        let down: Vec<(f64, usize, usize, f64)> = cands
            .iter()
            .filter_map(|&l| {
                let acc: Vec<&(f64, bool)> = 按条.iter().filter(|x| x.0 <= l).collect();
                if acc.is_empty() || l > 1.0 - delta {
                    return None;
                }
                let k = acc.iter().filter(|x| x.1).count();
                let u = jpp_value::stat::binomial_upper(k, acc.len(), conf_delta);
                (u <= alpha).then_some((l, acc.len(), k, u))
            })
            .collect();
        let mut best: Option<((f64, usize, usize, f64), (f64, usize, usize, f64))> = None;
        for a in &up {
            for d in &down {
                // lo = l + δ ≤ hi = h − δ
                if d.0 + delta > a.0 - delta {
                    continue;
                }
                if best
                    .as_ref()
                    .map(|(ba, bd)| a.1 + d.1 > ba.1 + bd.1)
                    .unwrap_or(true)
                {
                    best = Some((*a, *d));
                }
            }
        }
        let Some(((h, na, ka, ua), (l, nd, kd, ud))) = best else {
            let n_needed = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
            return Err(Refusal::认证不过(Certificate::Refused {
                best_ucb: 1.0,
                best_hi: 1.0,
                best_n_accepted: 0,
                n_needed,
            }));
        };
        let (hi, lo) = ((h - delta).clamp(0.0, 1.0), (l + delta).clamp(0.0, 1.0));
        let 规则 = "两侧联合选线：两区各自二项上界 ≤ α，取已决条数最多者（真值通道）".to_string();
        let upper = Cert {
            alpha,
            conf_delta,
            hi,
            n_accepted: na,
            n_errors: ka,
            ucb: ua,
            cluster_unit: "条".into(),
            resample: Some((0, 规则.clone())),
            cost: None,
            bounded_side: "上侧：界定 p ≥ hi + δ 一侧的假放行".into(),
            label_source: lsrc.clone(),
            label_fp: label_fp.clone(),
            selection: None,
            grade: CertGrade::Formal,
        };
        let lower = Cert {
            alpha,
            conf_delta,
            hi: lo,
            n_accepted: nd,
            n_errors: kd,
            ucb: ud,
            cluster_unit: "条".into(),
            resample: Some((0, 规则)),
            cost: None,
            bounded_side: "下侧：界定 p ≤ lo − δ 一侧的漏放行；hi 字段此处存的是 lo".into(),
            label_source: lsrc,
            label_fp,
            selection: None,
            grade: CertGrade::Formal,
        };
        let r = self.records.get_mut(key).expect("刚读过");
        r.certs.insert(upper.addr(), upper.clone());
        r.hi = hi;
        r.lo = lo;
        r.lower = Some(lower);
        r.status = "上岗".into();
        r.fixture = false;
        r.unsure_rate = 经验unsure率(&按条, &[], Some(Op::Test), hi, lo, Some(delta));
        r.unsure_rate_delta = Some(delta);
        Ok(upper)
    }

    /// **两侧联合认证，拆分样本版**（B24 的多重比较要求）。
    ///
    /// [`commission_two_sided`] 在同一批数据上既选线对又算二项上界；上界只对**固定**的线成立，
    /// 对「在同批数据上最大化已决条数后选出的线」不成立——候选越多，真实错误率越可能超 α。
    /// 这里把带标注的样本排成规范序（按 `(p, 真值)`），再按 `splitmix64(seed ^ 下标)` 的最低位分成两半：
    /// **选线半**上照原规则选出一对 `(h, l)`；**认证半**上对这一对只检验一次，两侧各一个二项上界。
    ///
    /// 认证半里任一侧的已决条数小于零错误所需条数（`n_needed_zero_error`）时，
    /// 如实停在待核（`跑不成`，原因以「待核」开头），不降低门槛。
    pub fn commission_two_sided_split(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
    ) -> Result<Cert, Refusal> {
        self.commission_two_sided_split_graded(key, alpha, conf_delta, seed, CertGrade::Formal)
    }

    /// 同上，证书写上认证等级（B72：导入时正式 α 不过再按试用 α 认证，证书记 `Trial`）。
    /// 算法与正式档完全相同，等级只是写进证书的一个事实。
    pub fn commission_two_sided_split_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
        grade: CertGrade,
    ) -> Result<Cert, Refusal> {
        let bad = |why: &str| Refusal::跑不成(why.to_string());
        if !(alpha > 0.0 && alpha < 1.0) || !(conf_delta > 0.0 && conf_delta < 1.0) {
            return Err(bad("alpha 与 conf_delta 必须在开区间 (0, 1)"));
        }
        let rec = self.records.get(key).ok_or_else(|| bad("没有这条记录"))?;
        if rec.status == "停岗" {
            return Err(bad("停岗的键不经由认证复岗"));
        }
        let 带标注: Vec<&Sample> = rec
            .samples
            .iter()
            .filter(|s| s.label.is_some() && s.p.is_some())
            .collect();
        if 带标注.is_empty() {
            return Err(bad("没有带标注的样本：线只从标注记录来"));
        }
        if 带标注.iter().any(|s| s.phys != "noul") {
            return Err(bad("两侧认证只对 test 题有定义"));
        }
        let delta = rec.delta.unwrap_or(self.profile.delta.0);
        let 按条: Vec<(f64, bool)> = 带标注
            .iter()
            .map(|s| (s.p.expect("已滤"), s.label == Some(1)))
            .collect();
        let label_fp = 标注集指纹(&带标注);
        let lsrc = rec.label_source.clone();
        // **分半不许随行序变**（Codex 评审 PR #28）：同一批 `(p, 真值)` 换个行序，
        // 按插入下标分半会分出不同的两半、可能改变认证结论，而 `label_fp` 对行序不敏感，
        // 证书地址与审计元数据却分不出这两次。先排成规范序（与指纹同一口径），再按下标分。
        // 样本带分层（B75 类记录）时按来源各自分半后合并。
        let (选线半, 认证半, 分层) = 分半(&带标注, seed);
        let n_needed = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
        let (正, 负) = (
            选线半.iter().filter(|x| x.1).count(),
            选线半.iter().filter(|x| !x.1).count(),
        );
        if 正.min(负) < n_needed {
            return Err(bad(&format!(
                "待核：选线半样本不足（正例 {正}、负例 {负}，零错误也需每侧 ≥ {n_needed}；选线半 {} 条、认证半 {} 条）",
                选线半.len(),
                认证半.len()
            )));
        }
        let (best, candidates) = 选两侧线对(&选线半, delta, alpha, conf_delta);
        let Some((h, l)) = best else {
            let n_needed = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
            return Err(Refusal::认证不过(Certificate::Refused {
                best_ucb: 1.0,
                best_hi: 1.0,
                best_n_accepted: 0,
                n_needed,
            }));
        };
        // 认证半：对选出的这一对只检验一次
        let up: Vec<&(f64, bool)> = 认证半.iter().filter(|x| x.0 >= h).collect();
        let down: Vec<&(f64, bool)> = 认证半.iter().filter(|x| x.0 <= l).collect();
        if up.len() < n_needed || down.len() < n_needed {
            return Err(bad(&format!(
                "待核：认证半样本不足（上侧已决 {}、下侧已决 {}，零错误也需每侧 ≥ {}；选线半 {} 条、认证半 {} 条）",
                up.len(),
                down.len(),
                n_needed,
                选线半.len(),
                认证半.len()
            )));
        }
        let ka = up.iter().filter(|x| !x.1).count();
        let kd = down.iter().filter(|x| x.1).count();
        let ua = jpp_value::stat::binomial_upper(ka, up.len(), conf_delta);
        let ud = jpp_value::stat::binomial_upper(kd, down.len(), conf_delta);
        if ua > alpha || ud > alpha {
            return Err(Refusal::认证不过(Certificate::Refused {
                best_ucb: ua.max(ud),
                best_hi: h,
                best_n_accepted: up.len() + down.len(),
                n_needed,
            }));
        }
        let sel = Selection {
            method: if 分层 { "split-strata" } else { "split" }.into(),
            seed,
            n_select: 选线半.len(),
            n_certify: 认证半.len(),
            candidates,
        };
        let (hi, lo) = ((h - delta).clamp(0.0, 1.0), (l + delta).clamp(0.0, 1.0));
        let 规则 = "两侧联合选线（拆分样本）：选线半上取两区上界各 ≤ α 且已决最多的一对，认证半上对该对各检验一次".to_string();
        let upper = Cert {
            alpha,
            conf_delta,
            hi,
            n_accepted: up.len(),
            n_errors: ka,
            ucb: ua,
            cluster_unit: "条".into(),
            resample: Some((0, 规则.clone())),
            cost: None,
            bounded_side: "上侧：界定 p ≥ hi + δ 一侧的假放行".into(),
            label_source: lsrc.clone(),
            label_fp: label_fp.clone(),
            selection: Some(sel.clone()),
            grade,
        };
        let lower = Cert {
            alpha,
            conf_delta,
            hi: lo,
            n_accepted: down.len(),
            n_errors: kd,
            ucb: ud,
            cluster_unit: "条".into(),
            resample: Some((0, 规则)),
            cost: None,
            bounded_side: "下侧：界定 p ≤ lo − δ 一侧的漏放行；hi 字段此处存的是 lo".into(),
            label_source: lsrc,
            label_fp,
            selection: Some(sel),
            grade,
        };
        let r = self.records.get_mut(key).expect("刚读过");
        r.certs.insert(upper.addr(), upper.clone());
        r.hi = hi;
        r.lo = lo;
        r.lower = Some(lower);
        r.status = "上岗".into();
        r.fixture = false;
        r.unsure_rate = 经验unsure率(&按条, &[], Some(Op::Test), hi, lo, Some(delta));
        r.unsure_rate_delta = Some(delta);
        Ok(upper)
    }
}

impl CalibStore {
    /// **K 元划分（`select` / `measure`）的单侧认证，拆分样本版**（B63）。
    ///
    /// 样本的 `p` 是胜出候选（或档位）的概率 p_max，`label` 是「argmax 是否等于真值」。
    /// 认证与 B24 上侧同形：选线半上取二项上界 ≤ α 且已决最多的 `h`，认证半上对它只检验一次；
    /// 记录的 `hi = h − δ`，于是 `cut` 的 `p_max ≥ hi + δ` 正好是 `p_max ≥ h`。
    /// K 元划分没有否定出口，`lo` 无消费者，记 0。
    pub fn commission_upper_split(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
    ) -> Result<Cert, Refusal> {
        self.commission_upper_split_graded(key, alpha, conf_delta, seed, CertGrade::Formal)
    }

    /// 同上，证书写上认证等级（B72：K 元单侧线同样按 α 分档）。
    pub fn commission_upper_split_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
        grade: CertGrade,
    ) -> Result<Cert, Refusal> {
        let bad = |why: &str| Refusal::跑不成(why.to_string());
        if !(alpha > 0.0 && alpha < 1.0) || !(conf_delta > 0.0 && conf_delta < 1.0) {
            return Err(bad("alpha 与 conf_delta 必须在开区间 (0, 1)"));
        }
        let rec = self.records.get(key).ok_or_else(|| bad("没有这条记录"))?;
        if rec.status == "停岗" {
            return Err(bad("停岗的键不经由认证复岗"));
        }
        let 带标注: Vec<&Sample> = rec
            .samples
            .iter()
            .filter(|s| s.label.is_some() && s.p.is_some())
            .collect();
        if 带标注.is_empty() {
            return Err(bad("没有带标注的样本：线只从标注记录来"));
        }
        let phys = 带标注[0].phys.clone();
        if 带标注.iter().any(|s| s.phys != phys) || !(phys == "choice" || phys == "score") {
            return Err(bad("K 元单侧认证只对同一种 select / measure 样本有定义"));
        }
        let op = if phys == "choice" {
            Op::Select
        } else {
            Op::Measure
        };
        let delta = rec.delta.unwrap_or(match op {
            Op::Select => self.profile.delta.1,
            _ => self.profile.delta.2,
        });
        let label_fp = 标注集指纹(&带标注);
        let lsrc = rec.label_source.clone();
        let mut 规范序: Vec<(f64, bool)> = 带标注
            .iter()
            .map(|s| (s.p.expect("已滤"), s.label == Some(1)))
            .collect();
        规范序.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let (选线半, 认证半, 分层) = 分半(&带标注, seed);
        let n_needed = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
        // 选线：h 取样本值与相邻中点；h − δ ≥ 0 才可表达
        let mut ps: Vec<f64> = 选线半.iter().map(|x| x.0).collect();
        ps.sort_by(|a, b| a.total_cmp(b));
        ps.dedup();
        let mut cands = ps.clone();
        for w in ps.windows(2) {
            cands.push((w[0] + w[1]) / 2.0);
        }
        cands.sort_by(|a, b| a.total_cmp(b));
        cands.dedup();
        let mut 满足 = 0usize;
        let mut best: Option<(f64, usize)> = None;
        for &h in &cands {
            if h < delta {
                continue;
            }
            let acc: Vec<&(f64, bool)> = 选线半.iter().filter(|x| x.0 >= h).collect();
            if acc.len() < n_needed {
                continue;
            }
            let k = acc.iter().filter(|x| !x.1).count();
            if jpp_value::stat::binomial_upper(k, acc.len(), conf_delta) <= alpha {
                满足 += 1;
                if best.map(|b| acc.len() > b.1).unwrap_or(true) {
                    best = Some((h, acc.len()));
                }
            }
        }
        let Some((h, _)) = best else {
            return Err(bad(&format!(
                "待核：选线半上没有满足 α 的单侧线（选线半 {} 条，零错误也需已决 ≥ {n_needed}）",
                选线半.len()
            )));
        };
        let up: Vec<&(f64, bool)> = 认证半.iter().filter(|x| x.0 >= h).collect();
        if up.len() < n_needed {
            return Err(bad(&format!(
                "待核：认证半样本不足（已决 {}，零错误也需 ≥ {n_needed}；选线半 {} 条、认证半 {} 条）",
                up.len(),
                选线半.len(),
                认证半.len()
            )));
        }
        let ka = up.iter().filter(|x| !x.1).count();
        let ua = jpp_value::stat::binomial_upper(ka, up.len(), conf_delta);
        if ua > alpha {
            return Err(Refusal::认证不过(Certificate::Refused {
                best_ucb: ua,
                best_hi: h,
                best_n_accepted: up.len(),
                n_needed,
            }));
        }
        let sel = Selection {
            method: if 分层 { "split-strata" } else { "split" }.into(),
            seed,
            n_select: 选线半.len(),
            n_certify: 认证半.len(),
            candidates: 满足,
        };
        let hi = (h - delta).clamp(0.0, 1.0);
        let cert = Cert {
            alpha,
            conf_delta,
            hi,
            n_accepted: up.len(),
            n_errors: ka,
            ucb: ua,
            cluster_unit: "条".into(),
            resample: Some((
                0,
                "K 元单侧选线（拆分样本，B63）：选线半上取上界 ≤ α 且已决最多的 h，认证半上检验一次".into(),
            )),
            cost: None,
            bounded_side: "上侧：界定 p_max ≥ hi + δ 时 argmax 错误的比例".into(),
            label_source: lsrc,
            label_fp,
            selection: Some(sel),
            grade,
        };
        let 全体: Vec<(f64, bool)> = 规范序.clone();
        let r = self.records.get_mut(key).expect("刚读过");
        r.certs.insert(cert.addr(), cert.clone());
        r.hi = hi;
        r.lo = 0.0;
        r.lower = None;
        r.status = "上岗".into();
        r.fixture = false;
        r.unsure_rate = 经验unsure率(&全体, &[], Some(op), hi, 0.0, Some(delta));
        r.unsure_rate_delta = Some(delta);
        Ok(cert)
    }
}

/// 在给定样本上选两侧线对（`commission_two_sided` 与拆分版共用的选线规则）。
/// 返回 `((h, l), 满足条件的候选对数)`；`h`、`l` 是判区边界（未扣 δ）。
fn 选两侧线对(
    按条: &[(f64, bool)],
    delta: f64,
    alpha: f64,
    conf_delta: f64,
) -> (Option<(f64, f64)>, usize) {
    let mut ps: Vec<f64> = 按条.iter().map(|x| x.0).collect();
    ps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ps.dedup();
    let mut cands: Vec<f64> = ps.clone();
    for w in ps.windows(2) {
        cands.push((w[0] + w[1]) / 2.0);
    }
    cands.sort_by(|a, b| a.partial_cmp(b).unwrap());
    cands.dedup();
    let up: Vec<(f64, usize)> = cands
        .iter()
        .filter_map(|&h| {
            let acc: Vec<&(f64, bool)> = 按条.iter().filter(|x| x.0 >= h).collect();
            if acc.is_empty() || h < delta {
                return None;
            }
            let k = acc.iter().filter(|x| !x.1).count();
            (jpp_value::stat::binomial_upper(k, acc.len(), conf_delta) <= alpha)
                .then_some((h, acc.len()))
        })
        .collect();
    let down: Vec<(f64, usize)> = cands
        .iter()
        .filter_map(|&l| {
            let acc: Vec<&(f64, bool)> = 按条.iter().filter(|x| x.0 <= l).collect();
            if acc.is_empty() || l > 1.0 - delta {
                return None;
            }
            let k = acc.iter().filter(|x| x.1).count();
            (jpp_value::stat::binomial_upper(k, acc.len(), conf_delta) <= alpha)
                .then_some((l, acc.len()))
        })
        .collect();
    let mut best: Option<(f64, f64, usize)> = None;
    let mut n = 0;
    for a in &up {
        for d in &down {
            if d.0 + delta > a.0 - delta {
                continue;
            }
            n += 1;
            if best.map(|b| a.1 + d.1 > b.2).unwrap_or(true) {
                best = Some((a.0, d.0, a.1 + d.1));
            }
        }
    }
    (best.map(|b| (b.0, b.1)), n)
}

/// 分半的一半：`(p, 真值)` 按规范序。
type 半 = Vec<(f64, bool)>;

/// 拆分认证的分半（B24；B75 分层）。每层内排成规范序（按 `(p, 真值)`），按
/// `splitmix64(seed ^ 层内下标)` 的最低位分两半，各层按来源名（BTreeMap 序）依次合并。
/// 没有任何样本带分层时只有一层，与分层之前的分半逐位相同。第三个返回值：是否分了层。
fn 分半(带标注: &[&Sample], seed: u64) -> (半, 半, bool) {
    type 按层 = std::collections::BTreeMap<Option<String>, Vec<(f64, bool)>>;
    let mut 层: 按层 = Default::default();
    for s in 带标注 {
        层.entry(s.stratum.clone())
            .or_default()
            .push((s.p.expect("已滤"), s.label == Some(1)));
    }
    let 分层 = 层.keys().any(|k| k.is_some());
    let (mut 选线半, mut 认证半) = (vec![], vec![]);
    for (_, mut xs) in 层 {
        xs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (i, x) in xs.iter().enumerate() {
            if splitmix64(seed ^ i as u64) & 1 == 0 {
                选线半.push(*x)
            } else {
                认证半.push(*x)
            }
        }
    }
    (选线半, 认证半, 分层)
}

/// 确定性分半用的混合函数（splitmix64）。
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod 分半测试 {
    use super::*;

    fn 样本(p: f64, l: u8, stratum: Option<&str>) -> Sample {
        Sample {
            p: Some(p),
            label: Some(l),
            perms: 0,
            mode_share: None,
            mode: LiteralMode::default(),
            phys: "noul".into(),
            cluster: None,
            stratum: stratum.map(String::from),
        }
    }

    /// 没有分层时与步 20f 之前的分半逐位相同（旧算法原样抄在这里对照）；带分层时按来源各自分半。
    // 依据：B24（拆分样本）；B75（分层分半）
    #[test]
    fn 无分层时分半不变() {
        let xs: Vec<Sample> = (0..57)
            .map(|i| 样本((i * 37 % 100) as f64 / 100.0, (i % 3 == 0) as u8, None))
            .collect();
        let refs: Vec<&Sample> = xs.iter().collect();
        let (a, b, 分层) = 分半(&refs, 20260923);
        let mut 规范序: Vec<(f64, bool)> = xs
            .iter()
            .map(|s| (s.p.unwrap(), s.label == Some(1)))
            .collect();
        规范序.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let (mut oa, mut ob) = (vec![], vec![]);
        for (i, x) in 规范序.iter().enumerate() {
            if splitmix64(20260923 ^ i as u64) & 1 == 0 {
                oa.push(*x)
            } else {
                ob.push(*x)
            }
        }
        assert_eq!((a, b, 分层), (oa, ob, false));
        let ys: Vec<Sample> = (0..40)
            .map(|i| {
                样本(
                    i as f64 / 40.0,
                    (i % 2) as u8,
                    Some(if i < 20 { "甲" } else { "乙" }),
                )
            })
            .collect();
        let refs: Vec<&Sample> = ys.iter().collect();
        let (a, b, 分层) = 分半(&refs, 7);
        assert!(分层);
        assert_eq!(a.len() + b.len(), 40);
    }
}
