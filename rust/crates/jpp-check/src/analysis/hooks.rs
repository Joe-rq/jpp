//! 规则钩子的分派（B71）：分析器在各钩子点把 `&Cx` 交给 `rules::RULES` 里的规则，收下它们返回的
//! 诊断。分派放在分析器这边，`rules/` 目录引用不到 `Checker`。

use crate::rules::{AnalysisId, CallSite, Cx, NamesAt, RULES};
use crate::*;

impl Checker<'_> {
    pub(crate) fn run_before(&mut self) {
        let cx = self.cx;
        for f in RULES.iter().filter_map(|r| r.hooks.before) {
            self.out.extend(f(&cx));
        }
    }

    pub(crate) fn run_after(&mut self) {
        let cx = self.cx;
        for f in RULES.iter().filter_map(|r| r.hooks.after) {
            self.out.extend(f(&cx));
        }
    }

    pub(crate) fn on_reading(&mut self, span: Span, msg: impl Into<String>, arith: bool) {
        let msg = msg.into();
        let cx = self.cx;
        for f in RULES.iter().filter_map(|r| r.hooks.reading) {
            self.out.extend(f(&cx, span, &msg, arith));
        }
    }

    pub(crate) fn on_scan_call(&mut self, scopes: &[Scope], name: &str, args: &[&Expr]) {
        let mut found = vec![];
        {
            let cx = Cx {
                names: Some(NamesAt {
                    readings: &self.readings,
                    scopes,
                }),
                ..self.cx
            };
            // 名字视图只给声明了 `Names` 的规则（`requires` 在这里生效，不只是登记）
            for r in RULES {
                let Some(f) = r.hooks.scan_call else { continue };
                if r.requires.contains(&AnalysisId::Names) {
                    found.extend(f(&cx, name, args));
                } else {
                    found.extend(f(&Cx { names: None, ..cx }, name, args));
                }
            }
        }
        self.out.extend(found);
    }

    pub(crate) fn on_call(&mut self, site: &CallSite) {
        let cx = self.cx;
        for f in RULES.iter().filter_map(|r| r.hooks.call) {
            self.out.extend(f(&cx, site));
        }
    }

    pub(crate) fn on_bind(&mut self, value: &Expr, name: &str, span: Span, block: &Block) {
        let cx = self.cx;
        for f in RULES.iter().filter_map(|r| r.hooks.bind) {
            self.out.extend(f(&cx, value, name, span, block));
        }
    }

    pub(crate) fn on_stmt(&mut self, e: &Expr) {
        let cx = self.cx;
        for f in RULES.iter().filter_map(|r| r.hooks.stmt) {
            self.out.extend(f(&cx, e));
        }
    }

    pub(crate) fn on_function(&mut self, f: &Function) {
        let cx = self.cx;
        for h in RULES.iter().filter_map(|r| r.hooks.function) {
            self.out.extend(h(&cx, f));
        }
    }
}
