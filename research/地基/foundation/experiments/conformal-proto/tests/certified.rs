//! 认证**成功**的那一侧：线是「最宽的、仍被认证住的那条」，与「上界最紧的那一格」**不是同一格**。
use conformal_probe::*;
fn 取(t: &str) -> Vec<(f64, bool)> {
    conformal_probe::ecal2(t)
}
#[test]
fn 认证成功时线取最宽而不是上界最紧() {
    for (t, alpha) in [("choice", 0.40), ("score", 0.40), ("noul", 0.45)] {
        let s = 取(t);
        match certify(&s, alpha, 0.10) {
            Certificate::Line { hi, n_accepted, n_errors, ucb } => {
                println!("{t} α={alpha}: 认证线 hi={hi:.3} 放行 {n_accepted}/{} 错 {n_errors} 上界 {ucb:.3}", s.len());
                assert!(ucb <= alpha);
            }
            Certificate::Refused { best_ucb, best_hi, best_n_accepted, .. } => {
                println!("{t} α={alpha}: 拒绝；最紧那一格 hi={best_hi:.3} 放行 {best_n_accepted} 上界 {best_ucb:.3}");
            }
        }
        // 对照：同一批数据上「上界最紧的那一格」在哪
        if let Certificate::Refused { best_hi, best_ucb, .. } = certify(&s, 0.01, 0.10) {
            println!("   （上界最紧那一格：hi={best_hi:.3} ucb={best_ucb:.3}）");
        }
    }
}
