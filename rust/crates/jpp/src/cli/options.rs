use std::path::PathBuf;

pub const HELP: &str = "J++ native source tools\nUsage:\n  jpp parse <file.jpp> [--ast]\n  jpp check <file.jpp> [--json] [--explain] [--input <file.json>] [--input-trusted] [--purpose <text>] [--mat <name>=<file>]... [--guard] [--release-on-declared] [--calib <calib-dir>] [--profile <profile.json>] [--questions-out <file.json>]\n  jpp run <file.jpp> [--json] [--explain] [--input <file.json>] [--input-trusted] [--purpose <text>] [--mat <name>=<file>]... [--mat-store <dir>] [--guard] [--release-on-declared] [--fixtures <file.json>] [--output <report.json>]\n          [--ledger-out <ledger.json>] [--replay <ledger.json> | --resume <ledger.json>] [--carry-in <file.json> [--carry-reauthorize]] [--carry-out <file.json>]
          [--profile <profile.json>] [--calib <calib-dir>] [--calib-out <calib-dir>]
          [--backend fixed|live|stub] [--model <name>] [--profiles-dir <dir>]\n          [--gen-model <m>] [--gen-profile <file>] [--cache <dir>]\n          [--trace-parent <traceparent> | --trace-seed <seed>] [--trace-label <label>]\n          [--confirm] [--confirm-above <usd>]\n  jpp calib-import <labels.jsonl> --calib-out <calib-dir> [--calib <calib-dir>] [--profile <profile.json>]\n          [--alpha 0.1] [--conf-delta 0.1] [--spot-check-min 0.9] [--spot-check-conf 0.95] [--abstain-warn 0.1] [--seed 20260923]\n          [--extent-min-disagree 3] [--extent-same-dir 0.8] [--extent-same-tier 0.667] [--scope-quantiles 0.01,0.99] [--scope-margins 2,0.10] [--class-min-sources 2] [--alpha-trial 0.25]\n          [--certify fixed-sequence|split|sequential] [--step <n>] [--cost fp,fn]\n          [--batch 10] [--order random|two-ends] [--coverage-target <tau>] [--mix-weights 0.8,0.1,0.05,0.05] [--from-ledger <ledger>]\n  jpp calib-import --from-ledger <ledger.jsonl> --key <key> --list-out <list.jsonl> [--materials <texts.json>] [--report <report.json>] [--seed <n>]\n  jpp calib-import <labels.jsonl> --calib <calib-dir> --extend-scope <key> --calib-out <calib-dir> [--alpha-trial 0.25] [--scope-quantiles 0.01,0.99] [--scope-margins 2,0.10]
  jpp calib-confirm <calib-dir> <key> --suspend|--keep\n  jpp profile check <profile.json>   (profile revision and delta by reading band; lists missing mid-band delta, exit 1)\n  jpp ledger-migrate <v2-ledger> <v3-out>\n  jpp ledger-tree <ledger>... [--json]   (merge ledgers by trace id into a call tree)\n  jpp bank <list|propose|diagnose|admit|reject|promote|split|merge|retire|reinstate|review> ... [--bank <dir>]   (question bank lifecycle; run `jpp bank` for the verbs' arguments)\n  jpp bank-stats <ledger>... [--bank <dir>] [--json]   (per-form usage statistics aggregated from ledgers)\n  jpp derive-admit <form_hash|slug> --rows <rows.jsonl> --split <method> --seed <n> [--derived-by <refine|elicit>] [--alpha <a>] [--conf-delta <d>] [--prereg <commit> --reviewer <name>] --reason <text> [--bank <dir>]   (B45 holdout gate for a derived question proposed with jpp bank propose --derived-by: not worse than the original on a held-out set disjoint from the scoring set, then admit; run `jpp derive-admit` for the row format)\n\nLeading relative imports load source libraries (resolved against the program file's directory); the read_json action resolves a relative path the same way first and falls back to the working directory, and if neither has the file its failure names both paths. --json makes diagnostics machine-readable, one JSON object each: {code, level, span {file, line, col, start, end}, message, fix, applicability manual|wiring|null, count}, plus explain for runtime codes E-rt-<name>; check --json prints one document {file, ok, errors, warnings, diagnostics} on stdout, run --json writes diagnostics as JSON lines (each starting with {) on stderr and leaves the report unchanged. Diagnostics with the same code, location and message are folded into one with a count (text output appends （同码同址 ×N）). --input binds the JSON file's value to the name input in the program (a host binding outside the program's outermost block; a program's own let input shadows it); its content is untrusted by default (every leaf, as for read_json), and the hash of the whole entry (name, canonical JSON and declared taint) goes into the ledger header as entry_hash, so a replay or resume must be given the same --input (a different one reports W-header: entry_hash). --input-trusted declares that file's content trusted (B108; requires --input, else a usage error); it only says the file comes from a source the host trusts, not that its content is correct, and it goes into entry_hash like any other declared taint, so replaying with a different --input-trusted state reports W-header: entry_hash. --purpose <text> binds the text to the name purpose (untrusted) and --mat <name>=<file> (repeatable) binds one material entry each under its name: a .json file is read as JSON, any other file as one UTF-8 text string; the material is untrusted with origin [\"input\"]; names must be identifiers other than input and purpose. Both go into entry_hash by the same algorithm (B105). run --mat-store <dir> installs a file material store, so lib/skeletons/select.jpp keeps its marks across runs (not installed under --replay, where transform results come from the ledger). By default J++ trusts the judge (B187). A cut with no usable line follows the judge's own answer (grade Answer in the report's exits): a yes/no question acts above 0.5 and ignores below it, select picks the most probable candidate, measure the most probable level, and an exact tie is unsure(tie). \"No usable line\" covers a key with no certified record, a suspended record, and cost or alpha with no matching certificate (those two also carry a J-15 untested carrier and a W-untested note). A line the author writes is the author's policy: cut(r, 0.7) reads as cut(r, {declare: {hi: 0.7}}), declare, stat and fit lines cut where written, and a calibrated line is used as before. stat: mass without declare follows the answer on the summed probability (above 0.5 act, below ignore, exactly 0.5 tie); stat expect or confidence, and a fit score, without a line are E-cut-options (there is no answer to follow on that scale). A calibrated line whose record has no delta uses the profile's delta (0 when the profile has none) and sets delta_unknown on the exit. Line-grade notes (W-fixture-line, W-form-line, W-class-line, W-trial-line, W-provisional, W-declared-line, W-calib-scope, W-scope-unknown, W-delta-unknown, W-suspend-candidate, W-lineage-unknown) are not printed; the same facts are in the report's exits rows. Nothing blocks a do and taint is recorded but gates nothing. An executor action (exec_py, check_tests, exec_sql) with no OS sandbox runs in a plain subprocess (same per-call temporary directory, static rejection list and network patch, no OS isolation) and check reports W-action-no-sandbox; it still counts as irreversible (B164). A program with an irreversible do run without --ledger-out writes its ledger next to the source file as <file name>.ledger.jsonl (<file name>.resumed.ledger.jsonl when --resume reads that same file); the path is printed first and the report carries ledger_path. Optional tools: --guard (run and check) turns on release gating: J-08 at check and run time (including the static subface and lineage), the line-grade notes above are printed again, and a program with an irreversible do must give --ledger-out (E-ledger-required). It goes into entry_hash (only when given, so hashes without it are unchanged), the IR and the report carry guard: true, and replaying or resuming with a different state reports W-header: entry_hash. Wherever this text says a line or an exit does not or cannot release an irreversible action, that holds only under --guard; without it the exits table's releases column is information only. Certification (calib-import, commission) is likewise an optional tool: no default path depends on it. --release-on-declared (run and check; B128) is the host accepting author-declared lines, cut(r, {declare: {hi, lo?}}) with or without stat, and only matters under --guard: their exits may then release irreversible do. It means \"I take responsibility for these lines\", not \"these lines are right\"; under --guard without it a declared-line exit routes but never releases an irreversible do, and check reports J-08 where every guard of such a do certainly comes from declared lines. Without --guard it is still accepted and recorded but changes nothing. It goes into entry_hash (only when given, so hashes without it are unchanged) and the report carries accept: {declared_lines: true}; replaying or resuming with a different state reports W-header: entry_hash. check accepts --input, --input-trusted, --guard and --release-on-declared only to know that input is bound, how it is declared, whether release gating is on and whether declared lines are accepted. --explain (check and run) prints the plan before any call is made. check prints it on stdout after the diagnostics, and prints it even when the check fails (for example E-budget-plan), so the reason for a rejection is visible; run prints it on stderr just before the run starts, so it also comes before a plan-time rejection (a static check error stops run before any plan is made; use check --explain then); with --json it is added as an explain object to check's one document or to run's report instead of text (only when given, so default outputs are unchanged; a run that stops with an error writes no report, use check --explain --json for that case). It shows the four estimates (calls including retries, judge layers, cost, latency) as a range, as a lower bound only (upper bound unknown), or as not estimated with the reason - never a made-up 0 - compares them with budget, lists each call site in source order with its layer lower bound and its maximum number of calls (execution count x sends per execution, retries included, merging not counted), the early registration the planner permits, the planner's rejection and warnings, and what the planner does not estimate yet (sinking, fission blocks, per-site token, cost and time, the unsure bound). Spend confirmation (11 section 5.5): a run that can spend money (not --replay; the judge has a price above 0 in its profile, or --gen-model is given) is refused with E-confirm-required, before any request, when its cost upper bound exceeds a threshold (default {CONFIRM_DEFAULT} USD; --confirm-above <usd> sets it) unless --confirm is given. The upper bound is the smaller of the effective budget cost and the planner's cost upper bound when it has one. The effective budget cost is the program's declared budget cost, or the carried balance's cost when --carry-in is tighter; on a resume whose --carry-in does not match the balance the ledger handed back, the carried side is the smaller of the ledger's remaining balance and the --carry-in, item by item. The report's confirm.upper_from says which one bound it: budget.cost (the program's declaration), carry (the carried balance was tighter) or plan (the planner's estimate was smaller); the declared budget cost is a ceiling, not a guarantee (the runtime stops once spending has passed it, so the last request can overshoot, and there is no token upper-bound estimate yet). Fixed observations and --replay never need --confirm; when the judge profile has no price (and there is no --gen-model) the cost cannot be converted to dollars, so the run goes ahead and --explain says the threshold was not checked. --explain shows the verdict; check --explain uses the default threshold. The basis line says what the numbers assume: check and a first run assume an empty ledger and no cache; run with --replay, --resume or --cache assumes lower bounds of 0. check --calib <calib-dir> loads calibration records (the same per-key JSON directory as run --calib) so the J-10 unsure bound runs at check time: when budget {unsure: u} is written, the sum of each question's certified unsure_rate is compared with u before any call and reported as a warning (a key with no usable record counts 1, the same as run without --calib); without --calib an empty record set is used. check --profile <profile.json> loads a model profile the same way run --profile does, so checks that read the profile (the B32 latency estimate, layers x p95 against budget.latency_p95, class-assumption downgrades and W-untested notes) give the same result at check time as before a run; without it check uses no profile. check also lists the calibration keys the program uses (K-088): each literal question label (test, select, measure) and each form(…, {calib}) with its form hash, the record level cut would use for it in the given --calib records (question, form or class; none means the question follows the judge's answer), each literal cost: [fp, fn] on a cut (only certificates made with calib-import --cost fp,fn serve it), and calls whose label is not a literal as skipped; text output prints this block on stderr after the diagnostics, check --json adds it as calib_keys {labels, uses, costs, skipped}. check --questions-out <file.json> writes the program's literal questions (test, select and measure whose question text, label and declarations are literals), grouped by calibration label, each with its zero-slot form hash, for migrating label-named calibration records (B116); it is written once the program lowers, even if the check then reports errors, and calls whose parts are not literals are listed as skipped with a reason. Material entries and purpose are given only through Session (the Rust API). Run defaults to fixed generation/judgment/response records; no model API requests are made. --backend live switches to the real JEV backend (JevClient::live), reading the API key only from ~/.typesafe-key (never logged or written to any report); it requires jpp to be built with `--features live` and is mutually exclusive with --fixtures. --backend stub is a deterministic stand-in judge (model stub-0) for checking that a program runs unchanged on a second judge: its readings come from a hash of the state and the question, it makes no requests, does not generate, and needs a profile like any backend (the repository keeps a stand-in profile at tests/profile_swap/stub-0.json; it is not a measurement). --model names the model for --backend live (default jev-1.13.0). --gen-model <m> (with --backend live|stub, jpp built with --features live; not with --fixtures or --replay) replaces the placeholder gen port with the claude -p generator (claude -p --model <m>): each gen call runs one subprocess in a worker pool (submit returns at once, poll checks completion; pool size from the generator profile's gen.concurrency, default 4) and its reply must be a JSON array of exactly n items; a timeout, a non-zero exit, a malformed or an empty reply becomes a Fail value recorded in the ledger, and generated materials are untrusted unless the generator profile says gen.taint_out trusted. The generator profile (gen-claude-p.json) is found like the judge profile: --gen-profile <file>, else --profiles-dir <dir>, else profiles/ next to the executable; its hash is printed and reported as gen_backend. A gen call is registered and sent with its layer at the next refresh point (together with pending judgments; the generator runs while the program goes on); the result is waited for only where it is first read (the same inspection points as a lazy cut), so several independent gen calls run at once and judgments in the same layer overlap with generation; ledger entries are written per layer in registration order, never completion order, and a gen that never reaches a refresh point is not sent. --cache <dir> reuses earlier results across runs (build once, then run daily): the ledgers in that directory (not its subdirectories; files that are not ledgers are skipped; a missing directory is an empty cache; any other read error stops with E-cache) are indexed, and a judge, gen or transform whose cache key matches is not sent again. The cache key leaves out the call site: a judge is keyed by model, state, question, physical form and render version (a select question also by its permutation seed), a gen by generator model, prompt, context hashes, n and retry_seq, a transform by its method, captured environment and arguments; do and ask are never reused. A hit is written to this run's ledger as a reused entry (reused_from, cost 0), so the ledger alone still replays; failures and reused entries are not used as sources. The same keys are also reused within one run once the first result has arrived. Replay never consults the cache. With --cache, or when anything was reused, the report gains cache {hits, same_run, cross_run, saved_calls, requests}. With --gen-model the generator model and its profile hash go into the ledger header (gen_model, gen_profile_hash) and are compared like the judge profile. --carry-in <file.json> passes a whole-session balance down a chain of runs (C-3): the file holds {calls, cost, latency_p95, escalate, hop, round, depth_cap}; this run's limits become the smaller of the program's budget and that balance, item by item (latency_p95 is the judge-call latency budget, not a wall-clock deadline), and hop counts the cross-program chain (recursion depth is counted per run from 0). When the balance runs out the run stops sending exactly as when its own budget runs out (W-budget, unsure(budget), the program still returns); when hop has reached depth_cap the run still evaluates but sends nothing (unsure(depth) per question, gen/do give failure values, ask gives unsure(depth); budget.cause depth, W-budget). The balance goes into the ledger header, and --replay takes it from there, so --carry-in cannot be combined with --replay. The report gains carry, the balance left for the next run (spent calls, cost, latency and asks taken off, hop plus one, round back to 0); --carry-out <file.json> (requires --carry-in, or --replay) writes the same balance to a file that the next run can take as --carry-in; it is worked out from the round's Spent ledger entry, so it is also written when the run fails with an error (a round that started but left no Spent entry hands back nothing), and a replay writes it byte for byte as the first run did. The planner's reliable lower bound checks the effective limits, so a balance already below it stops the run with E-budget-plan before any call, and the balance is handed back unchanged. On a resume (the ledger header already holds this round's balance) the run expects the balance the previous trip handed back: any other --carry-in is treated as an old file (W-carry) and the run is limited by the ledger's handed-back balance instead; --carry-reauthorize (requires --carry-in) is the explicit way to grant a new balance, which starts a new segment from this trip (W-carry-reauth says what the previous segment spent and where the new one starts). A resume without --carry-in on a ledger that holds a balance reports W-carry-missing and runs without the whole-session cap. A balance can only shrink along the chain; a larger one comes only from the host writing a new file and re-authorizing. Registered actions: {actions}. --profile loads a model profile (lines, deltas, windows, prices, class-assumption fields). A live run (first run or --resume) must have a profile (B73): --profile <file>, else --profiles-dir <dir>/<model>.json, else profiles/<model>.json next to the jpp executable (releases ship profiles/); if none is found the run stops with E-profile-missing listing the paths tried, and never falls back to code defaults. The profile hash goes into the ledger header, and the price comes only from the profile's cost field (no price: cost is reported as Unknown with W-cost-unknown). Replay makes no requests. A certificate records the delta it was certified with (B104) and cut uses that delta (B187: the older stricter-of-two rule and W-delta-mismatch are retired); a line whose record has no delta uses the profile's delta and sets delta_unknown on the exit. To reproduce a live run byte for byte, replay with the same profile. A fixed-observation run without a profile still runs until step 15d and always prints that no profile is loaded and lines and deltas are code defaults. --calib loads calibration records from a directory of per-key JSON files. --calib-out folds this run's readings into those records and writes them back, which is the only way the calibration loop closes: J-03 forbids a program from writing a line itself. File paths use the working directory. Resume performs only actions that have no intent record; replay rejects unrecorded actions. Before an irreversible action runs, an intent record is written to the ledger file (--ledger-out, or the default path) and flushed to disk, and its result is flushed right after it returns; other entries are flushed at the end of each layer. If a run stops after the intent but before the result (for example the process is killed), --resume does not perform that action again: the program gets a failure value marked unknown_outcome (judged as unsure(fail)) and W-unknown-outcome, unless the profile declares the action idempotent. A run or resume of a program with an irreversible do (a do whose action name is not a literal counts) without --ledger-out writes its ledger next to the source file as <file name>.ledger.jsonl (<file name>.resumed.ledger.jsonl when --resume reads that same file) and prints the path; under --guard it must give --ledger-out instead, otherwise it stops before any effect with E-ledger-required; replay does not need either. Replay restores the calibration records the ledger recorded as used when they are not supplied again. Ledgers are format v3 (the calibration records a run used are CalibUsed entries, the last one per key counts); a v2 ledger is migrated in memory when read (a note is printed, the file is not rewritten), and ledger-migrate rewrites it as v3. calib-import is the truth channel: it folds labelled readings (JSONL: key or form, item, p, label true|false|\"ambiguous\", source human|computed|model:<name>, optional spot_check review-batch id, optional generator, optional q and fill, optional kind or slot_shape one|pair with over_kind; conflicting kinds on one key are rejected with E-kind-conflict) into calibration records and certifies them two-sided. --certify picks the method (new imports only; existing records are left as they are). --cost fp,fn (B129) certifies a cost line instead: the labelled rows are split in half (B85 stratified alternating split, same as split; --seed picks where each segment starts), the line minimizes fp × false releases + fn × missed releases on the selection half, and the certificate checks that line against --alpha (then --alpha-trial) on the certification half only, not the rows used to pick it; it takes test rows only, is one-sided (below the line exits are unsure(band), not ignore), cannot be combined with --certify, --order, --step, --batch, --coverage-target, --mix-weights, --extend-scope or --list-out, and is refused on a key that already holds a certificate not made from a cost; cut(r, {cost: [fp, fn]}) uses exactly that certificate. Because certification only sees half the labelled rows, roughly twice as many are needed to reach the same sample-size floor as an unsplit line. fixed-sequence (default, B86) does not split the rows: candidate thresholds come from the readings alone (the upper side's j-th candidate is the lower edge of the n_needed + j*s highest readings, ties widened to the group edge, taking the group value and the midpoint to the next value; the lower side mirrors it; s = --step, default 5% of the rows, at least 1), they are tested from strictest to widest with the Clopper-Pearson bound, testing stops at the first failure, and the pair with the most decided rows is taken from the two passing prefixes; the family error per side stays at most --conf-delta, the same level as split certification, without spending half the rows. (Choosing the widest passing threshold on the same rows without this order is not proven; on nested threshold families its measured inflation is only about twofold, so most of what the split's discarded half bought was a guarantee with a proof, not protection from gross overfitting — the fixed order gives the proof for free.) The certificate records the method, the candidate rule version, s, delta, how many candidates each side generated and where it stopped, so a later load can rerun it from the labels. split (B85) is the older split-sample method, now with a stratified alternating split: in canonical order the negative rows and the positive rows each alternate between the selection half and the certification half, --seed only picks where each segment starts (per source for class rows); a pair is chosen on one half and certified once on the other. sequential (B87) lets you label in batches and import as you go (label, import, and stop when it is enough): rows arrive in --batch sized batches, every candidate threshold keeps a mixture e-process (alternatives 0, alpha/4, alpha/2, 3alpha/4 weighted by --mix-weights), a candidate is rejected once its e-value reaches 1/--conf-delta, which stays valid under any stopping time; by default it stops where labelling more could not widen the line (settled; the pair is never wider than fixed-sequence's, and a zero-error side needs 24 rows at alpha 0.1, 10 at 0.25), or at the narrowest legal pair whose coverage of the sampled readings reaches --coverage-target; if it has not stopped the gate reports how many rows are labelled, each side's e-value, the current and reachable coverage and roughly how many more zero-error rows are needed. The arrival order is random (a seeded permutation, --order random) unless the rows come from a to-label list. --from-ledger <ledger> --key <key> --list-out <list> writes that list (B88) from a first run's ledger: every reading of that key is the sampling frame, rows are grouped from both ends inward (group upper1, lower1, upper2, ..., rest; random within a group, seeded), and each row carries only item (the material's state hash), q (the question's hash: one material can be asked several questions, B107), group and, with --materials, the material text, with --report <report.json> (the run's report) the question's template and fill - never the reading or an exit, so the labeller cannot lean on the judge; a key asked with more than one question and no --report warns W-list-no-question. Importing labels with --from-ledger joins each row's reading back from the ledger by (item, q) (a row without q whose item has more than one reading under that key is rejected with E-list-ambiguous), and with --report takes the question kind from the report (B120), defaults to --certify sequential with the two-ends order, and requires the labelled rows to be a prefix of the list; with that order only the first group's candidates are judged on partial labels, a wider candidate is judged only once all of its rows are labelled. A sequential import must give all labelled rows of the key at once (import the cumulative file each time, without --calib). truth per item is taken in the order computed > human > review row > annotation row (B36; a review row is any row carrying spot_check, human or model:<name>; a review from the same source as that item's annotation or from the material generator is rejected); model-only truth is certified only when a same-key review batch reaches --spot-check-min, and the gate names model reviewers: the point estimate below it keeps the record pending; a point estimate at or above it whose one-sided --spot-check-conf lower bound is still below it certifies the line provisionally (gate \"临时上岗\", W-provisional at use) and reports how many more all-agreeing checks would confirm it. When review disagreements reach --extent-min-disagree and their direction agrees at --extent-same-dir or higher (or, with fill_tier on the rows, --extent-same-tier or more fall in one fill tier), the question's scope is judged undetermined (B36 5(c)): the record stays pending with the reason in the gate and no further review is requested; rewrite the question first. select and measure rows (B63) add op select|measure (or use a form with that op), p = the winning candidate's or level's probability, pick = the reading's argmax index, and label = the true candidate index (over order) or level index (scale order); they are certified one-sided on p_max (Pick/At only when p_max >= hi + delta; there is no low side). Two certification grades (B72): the key is first certified at --alpha (formal grade); only if that fails (certification refused or too few rows) is it certified again at --alpha-trial (default 0.25; a value not above --alpha disables it), and the certificate is marked trial. A trial line routes act/ignore like any line but reports W-trial-line and never releases an irreversible action; a trial import never overwrites a key that already holds a certified formal line. When model labels supply the truth (B89) the certificate records alpha_eff, the false-release bound relative to the reviewer: alpha when review rows cover every certified row in the decided region, alpha + (1 - a_lb) when the reviewed rows inside the decided region give the one-sided agreement lower bound a_lb, alpha + (1 - a_lb)/c for older records whose review batch was not drawn from that region (c = the share of certified rows in it); a line whose alpha_eff exceeds its alpha is graded trial (routes, never releases an irreversible action), and the record names its truth baseline (model:<reviewer> or human). To put model labels on a formal line, have the review cover the certification set. Review rows (rows with spot_check) must not carry p, pick, exit or reading - the reviewer may not see the judge's answer - and are rejected with E-review-leak; their reading is joined from the same item's annotation row. Sample size (fixed-sequence; each side's first candidate needs 22 zero-error decided rows at alpha 0.1 and 9 at alpha 0.25): a formal line (may release an irreversible do) needs about 60 labelled rows for a literal question form and about 60–80 for a semantic one — a form whose scope is undetermined should be rewritten before labelling; a trial line (routes only) needs about 32 rows, about 40 for a form whose readings are spread out. --certify split needs about 2–3 times as many. Measured offline: 地基/评估/2026-09-24-新题标注门槛-对照/results.md and 裁定复算/recompute.out.txt. When every certified row carries text (the judged material), the record stores a material fingerprint of the certification set (B68: character count, Chinese / Latin / digit / punctuation-and-space ratios, line count, mean line length, each as a --scope-quantiles interval widened by --scope-margins k,m: count-like quantities are divided/multiplied by k, ratios are widened by m and clipped to [0, 1]; the import prints how many certification rows fall outside their own range, which must be 0, else W-scope-self); at run time a line used on material outside that range still routes but reports W-calib-scope and cannot release an irreversible action. --extend-scope <key> (B91) extends that range to a new material style: label rows of the new style (p, label, text on every row) test the record's existing line pair once per side, without choosing a new line; each side needs as many zero-error decided rows as certification does (22 formal, 9 trial) and a binomial bound at most alpha, first at the record's alpha, else at --alpha-trial. If it passes, the batch's fingerprint is added to the record's scope.extensions; exits on material in an extension no longer count as out of scope, and a trial-level extension grades them Trial (W-scope-extension). A 10-row review never clears out-of-scope. Re-importing the key replaces the line and drops its extensions. A record without a fingerprint has an unknown scope (B104): its exits still route but never release an irreversible action (W-scope-unknown); when only some certified rows carry text, the fingerprint is built from those rows and the record notes how many (n_text). A certificate whose line was shifted by the certification bandwidth but that did not record it (an old split-sample certificate) likewise routes but never releases (W-delta-unknown) until it is re-imported or a later load reruns it. A row with class <label> (B34) goes to the class record of that calibration class instead of its own key; the sample's source is the row's form (its form hash), else the hash of its question text (a hand-written question), else its key — different fills of one form are one source (B75). A class record is certified only when the batch mixes at least --class-min-sources distinct sources and each source has at least the grade's zero-error row count (22 formal, 9 trial); under --certify split the halves are also stratified by source, and the record lists its sources (else it stays pending with the reason in the gate). At run time a question whose own key and form have no certified line borrows the class line of the key it was written with (W-class-line); a class line routes but does not release an irreversible action (B75). calib-confirm is the human confirmation of a suspension candidate (B25): a drift signal marks a certified line as a candidate (its exits still route but cannot release an irreversible action), --calib-out writes the candidate status, and --suspend or --keep settles it.";

/// 帮助文本：[`HELP`] 模板里的 `{actions}` 填入动作表的清单（比赛块 C-1：动作清单与注册同源）。
pub fn help() -> String {
    HELP.replace("{actions}", &jpp::actions::usage_list())
        .replace("{CONFIRM_DEFAULT}", &format!("{CONFIRM_DEFAULT_USD:.2}"))
}

// 默认模型名步 15g-0 起在注册表条目里（`jpp::backends::jev::SPEC.default_model`）。

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Parse {
        source: PathBuf,
        ast: bool,
    },
    Check {
        source: PathBuf,
        input: Option<PathBuf>,
        /// 宿主声明 `--input` 可信（步 14b-1，B108）：只在给了 `input` 时有意义
        input_trusted: bool,
        /// 宿主接受作者声明线放行（步 20j-2，B128）：J-08 静态子面据此不把声明线出口当作不放行
        release_on_declared: bool,
        /// 宿主开启放行把关（意图汇编 11a，`--guard`）：J-08 静态子面只在开时报
        guard: bool,
        /// 校准记录目录（`--calib`，L7 2026-09-28）：J-10 静态面（K-084/K-160）要各键的 `unsure_rate`，
        /// 校准键清单（K-088）据它报每道题命中哪一层。不给时按空记录本查，与 `run` 不给 `--calib` 同口径
        calib: Option<PathBuf>,
        /// 能力画像文件（`--profile`，L7 2026-09-28）：依赖画像的检查（B32 时延预算的静态面 `W-latency`、
        /// 类假设降级、`W-untested` 分档）在 `check` 上与 `run` 同口径；Z0157 起 `check` 还过一遍规划器，
        /// 可靠下界超预算报 `E-budget-plan`。不给时照旧不带画像
        profile: Option<PathBuf>,
    },
    Run(RunOptions),
    CalibImport(ImportArgs),
    CalibConfirm {
        dir: PathBuf,
        key: String,
        suspend: bool,
    },
    /// 账本 v2 → v3（步 18a，B124 Q3）
    LedgerMigrate {
        from: PathBuf,
        to: PathBuf,
    },
    /// 把几份账本按追踪编号拼成调用树并打印（C-2）；`--json` 出机器可读的树
    LedgerTree {
        ledgers: Vec<PathBuf>,
    },
    /// 题库生命周期与使用统计（步 27，B48）：`bank <动词> …` 与 `bank-stats <账本>…`；动词后的参数原样交给 `cli/bank.rs`
    Bank {
        stats: bool,
        args: Vec<String>,
    },
    /// 出题机制 derive 的留出比较与上岗（步 28，B45，主控:B0468）：`derive-admit …` 的参数原样交给 `cli/derive_admit.rs`
    DeriveAdmit {
        args: Vec<String>,
    },
    /// 画像自检（裁定四十五）：列版本与 δ 各读数段，缺中段 δ 报缺项
    ProfileCheck {
        profile: PathBuf,
    },
}

#[derive(Debug, PartialEq)]
pub struct ImportArgs {
    /// 标注文件；只导出待标清单（`--list-out`）时没有
    pub labels: Option<PathBuf>,
    pub calib: Option<PathBuf>,
    /// 导入时必给；只导出清单时没有
    pub calib_out: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    pub alpha: f64,
    pub conf_delta: f64,
    pub spot_check_min: f64,
    pub spot_check_conf: f64,
    pub abstain_warn: f64,
    pub seed: u64,
    pub extent_min_disagree: usize,
    pub extent_same_dir: f64,
    pub extent_same_tier: f64,
    pub scope_quantiles: (f64, f64),
    pub scope_margins: (f64, f64),
    pub class_min_sources: usize,
    pub alpha_trial: f64,
    /// B86 / B85：`fixed-sequence`（缺省）或 `split`
    pub certify: jpp::truth::CertifyMethod,
    /// B86：固定序步长；`None` = 池的 5%
    pub step: Option<usize>,
    /// 显式给了 `--certify`（`--from-ledger` 回填时缺省改为 sequential）
    pub certify_explicit: bool,
    /// B87：每批条数、顺序（random / two-ends）、覆盖目标、混合权重
    pub batch: usize,
    pub order: Option<String>,
    pub coverage_target: Option<f64>,
    pub weights: [f64; 4],
    /// B88：首跑账本（抽样框）、键、待标清单输出、材料文本（JSON 字符串数组）
    pub from_ledger: Option<PathBuf>,
    pub key: Option<String>,
    pub list_out: Option<PathBuf>,
    pub materials: Option<PathBuf>,
    /// B107（步 20h-2）：首跑的报告（`questions` 表）：清单行附题面与填法，回填时标注行附题类（B120 (a)）
    pub report: Option<PathBuf>,
    /// B91（步 20d-2）：范围扩展认证的键（`--calib` 里已有的认证线）
    pub extend_scope: Option<String>,
}

fn parse_import(args: &[String]) -> Result<Command, String> {
    let labels = args
        .get(1)
        .filter(|s| !s.starts_with("--"))
        .map(PathBuf::from);
    let (mut calib, mut calib_out, mut profile) = (None, None, None);
    let (mut alpha, mut conf_delta, mut spot_check_min, mut abstain_warn) = (0.1, 0.1, 0.9, 0.1);
    let mut spot_check_conf = 0.95;
    let mut seed: u64 = 20260923;
    let (mut extent_min_disagree, mut extent_same_dir, mut extent_same_tier) =
        (3usize, 0.8, 2.0 / 3.0);
    let mut scope_quantiles = (0.01, 0.99);
    let mut scope_margins = (2.0, 0.10);
    // B75：类记录「混合样本」的来源数下限（来源 = 题式，填法不算不同来源）
    let mut class_min_sources = 2usize;
    // B72：试用 α（正式 α 不过时再认证一次；不大于 --alpha 即不试）
    let mut alpha_trial = 0.25;
    // B86：缺省固定序认证（不拆分）；B85：split 为分层交替分半；序贯（B87）落步 20h
    let mut certify = jpp::truth::CertifyMethod::FixedSequence;
    let mut step: Option<usize> = None;
    let mut certify_explicit = false;
    // B87：每批 10 条、混合权重 (0.8, 0.1, 0.05, 0.05) 于 p₁ ∈ {0, α/4, α/2, 3α/4}；缺省停在 settled
    let (mut batch, mut order, mut coverage_target) = (10usize, None::<String>, None::<f64>);
    let mut weights = [0.8, 0.1, 0.05, 0.05];
    // B88：首跑账本作抽样框
    let (mut from_ledger, mut key, mut list_out, mut materials) = (None, None, None, None);
    let mut report = None;
    let mut extend_scope = None;
    // B129（步 20a-2a）：`--cost fp,fn` 是一种认证方式（代价线）；给了哪些与它无关的认证参数要记下来，
    // 同给即用法错误（静默忽略就是吞字段）
    let mut cost: Option<(f64, f64)> = None;
    let mut 给了: Vec<String> = vec![];
    let mut i = if labels.is_some() { 2 } else { 1 };
    while i < args.len() {
        let v = args
            .get(i + 1)
            .filter(|s| !s.starts_with("--"))
            .ok_or_else(|| format!("{} requires a value", args[i]))?;
        let num = |x: &str| {
            x.parse::<f64>()
                .map_err(|_| format!("{} expects a number, got {x}", args[i]))
        };
        给了.push(args[i].clone());
        match args[i].as_str() {
            "--calib" => calib = Some(PathBuf::from(v)),
            "--calib-out" => calib_out = Some(PathBuf::from(v)),
            "--profile" => profile = Some(PathBuf::from(v)),
            "--cost" => {
                let bad = || {
                    format!(
                        "--cost expects two positive numbers fp,fn (the cost of a false release and of a missed one), got {v}"
                    )
                };
                let (a, b) = v.split_once(',').ok_or_else(bad)?;
                let (a, b) = (
                    a.trim().parse::<f64>().map_err(|_| bad())?,
                    b.trim().parse::<f64>().map_err(|_| bad())?,
                );
                if !(a > 0.0 && b > 0.0 && a.is_finite() && b.is_finite()) {
                    return Err(bad());
                }
                cost = Some((a, b));
            }
            "--alpha" => alpha = num(v)?,
            "--conf-delta" => conf_delta = num(v)?,
            "--spot-check-min" => spot_check_min = num(v)?,
            "--spot-check-conf" => spot_check_conf = num(v)?,
            "--abstain-warn" => abstain_warn = num(v)?,
            "--extent-min-disagree" => {
                extent_min_disagree = v.parse::<usize>().map_err(|_| {
                    format!("--extent-min-disagree expects a non-negative integer, got {v}")
                })?
            }
            "--extent-same-dir" => extent_same_dir = num(v)?,
            "--extent-same-tier" => extent_same_tier = num(v)?,
            "--scope-quantiles" => {
                let bad = || {
                    format!(
                        "--scope-quantiles expects two numbers lo,hi in [0, 1] with lo < hi, got {v}"
                    )
                };
                let (a, b) = v.split_once(',').ok_or_else(bad)?;
                let (a, b) = (
                    a.trim().parse::<f64>().map_err(|_| bad())?,
                    b.trim().parse::<f64>().map_err(|_| bad())?,
                );
                if !(0.0 <= a && a < b && b <= 1.0) {
                    return Err(bad());
                }
                scope_quantiles = (a, b);
            }
            "--scope-margins" => {
                let bad =
                    || format!("--scope-margins expects k,m with k >= 1 and 0 <= m < 1, got {v}");
                let (a, b) = v.split_once(',').ok_or_else(bad)?;
                let (a, b) = (
                    a.trim().parse::<f64>().map_err(|_| bad())?,
                    b.trim().parse::<f64>().map_err(|_| bad())?,
                );
                if !(a >= 1.0 && (0.0..1.0).contains(&b)) {
                    return Err(bad());
                }
                scope_margins = (a, b);
            }
            "--class-min-sources" => {
                class_min_sources = v.parse::<usize>().map_err(|_| {
                    format!("--class-min-sources expects a non-negative integer, got {v}")
                })?
            }
            "--alpha-trial" => alpha_trial = num(v)?,
            "--certify" => {
                certify_explicit = true;
                certify = match v.as_str() {
                    "fixed-sequence" => jpp::truth::CertifyMethod::FixedSequence,
                    "split" => jpp::truth::CertifyMethod::Split,
                    "sequential" => jpp::truth::CertifyMethod::Sequential,
                    other => {
                        return Err(format!(
                            "--certify expects fixed-sequence|split|sequential, got {other}"
                        ));
                    }
                }
            }
            "--step" => {
                step = Some(
                    v.parse::<usize>()
                        .ok()
                        .filter(|n| *n >= 1)
                        .ok_or_else(|| format!("--step expects a positive integer, got {v}"))?,
                )
            }
            "--batch" => {
                batch = v
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n >= 1)
                    .ok_or_else(|| format!("--batch expects a positive integer, got {v}"))?
            }
            "--order" => {
                if v != "random" && v != "two-ends" {
                    return Err(format!("--order expects random|two-ends, got {v}"));
                }
                order = Some(v.clone());
            }
            "--coverage-target" => {
                let t = num(v)?;
                if !(0.0..=1.0).contains(&t) {
                    return Err(format!(
                        "--coverage-target expects a number in [0, 1], got {v}"
                    ));
                }
                coverage_target = Some(t);
            }
            "--mix-weights" => {
                let bad = || {
                    format!("--mix-weights expects four non-negative numbers summing to 1, got {v}")
                };
                let xs: Vec<f64> = v
                    .split(',')
                    .map(|x| x.trim().parse::<f64>())
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad())?;
                if xs.len() != 4
                    || xs.iter().any(|x| *x < 0.0)
                    || (xs.iter().sum::<f64>() - 1.0).abs() > 1e-9
                {
                    return Err(bad());
                }
                weights = [xs[0], xs[1], xs[2], xs[3]];
            }
            "--from-ledger" => from_ledger = Some(PathBuf::from(v)),
            "--key" => key = Some(v.clone()),
            "--list-out" => list_out = Some(PathBuf::from(v)),
            "--materials" => materials = Some(PathBuf::from(v)),
            "--report" => report = Some(PathBuf::from(v)),
            "--extend-scope" => extend_scope = Some(v.clone()),
            "--seed" => {
                seed = v
                    .parse::<u64>()
                    .map_err(|_| format!("--seed expects a non-negative integer, got {v}"))?
            }
            other => return Err(format!("unknown calib-import option '{other}'")),
        }
        i += 2;
    }
    if let Some((fp, fn_)) = cost {
        // 依据：B129（代价线是自己的认证方式；这些参数对它无消费者）
        const 不相干: [&str; 8] = [
            "--certify",
            "--order",
            "--step",
            "--batch",
            "--coverage-target",
            "--mix-weights",
            "--extend-scope",
            "--list-out",
        ];
        if let Some(o) = 给了.iter().find(|o| 不相干.contains(&o.as_str())) {
            return Err(format!(
                "--cost cannot be combined with {o}: a cost line is its own certification method (B129)"
            ));
        }
        certify = jpp::truth::CertifyMethod::Cost(fp, fn_);
        certify_explicit = true;
    }
    if list_out.is_some() {
        if from_ledger.is_none() || key.is_none() {
            return Err(
                "calib-import --list-out requires --from-ledger <ledger> and --key <key>".into(),
            );
        }
    } else {
        if labels.is_none() {
            return Err("calib-import requires a labels file (JSONL)".into());
        }
        if calib_out.is_none() {
            return Err("calib-import requires --calib-out <dir>".into());
        }
    }
    Ok(Command::CalibImport(ImportArgs {
        labels,
        calib,
        calib_out,
        profile,
        alpha,
        conf_delta,
        spot_check_min,
        spot_check_conf,
        abstain_warn,
        seed,
        extent_min_disagree,
        extent_same_dir,
        extent_same_tier,
        scope_quantiles,
        scope_margins,
        class_min_sources,
        alpha_trial,
        certify,
        step,
        certify_explicit,
        batch,
        order,
        coverage_target,
        weights,
        from_ledger,
        key,
        list_out,
        materials,
        report,
        extend_scope,
    }))
}

/// 观察后端：默认固定观察（不越界）；其余取值来自后端注册表（步 15g-0，`jpp::backends::REGISTRY`；
/// `live` 接真机 `JevClient`）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Backend {
    #[default]
    Fixed,
    Registered(&'static jpp::backends::BackendSpec),
}

impl Backend {
    /// 注册后端的条目；固定观察为 `None`
    pub fn spec(&self) -> Option<&'static jpp::backends::BackendSpec> {
        match self {
            Backend::Fixed => None,
            Backend::Registered(s) => Some(s),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct RunOptions {
    pub source: PathBuf,
    pub fixtures: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub ledger_out: Option<PathBuf>,
    pub replay: Option<PathBuf>,
    pub resume: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    pub calib: Option<PathBuf>,
    pub calib_out: Option<PathBuf>,
    pub backend: Backend,
    pub model: Option<String>,
    /// 真机画像目录（B73）：没给 `--profile` 时按 `<目录>/<model>.json` 找；缺省为可执行文件旁的 `profiles/`。
    pub profiles_dir: Option<PathBuf>,
    /// 宿主入口材料（步 14b-0）：JSON 文件，以名字 `input` 绑定给程序，叶子 untrusted
    pub input: Option<PathBuf>,
    /// 宿主声明 `--input` 可信（步 14b-1，B108）：只在 `input` 给了时有意义
    pub input_trusted: bool,
    /// 料库目录（B0472，`--mat-store`）：装上文件料库，选料标记落盘、跨运行复用；只凭账本重放不装。
    /// 由 [`take_entry`] 在解析前取出、解析后填进来
    pub mat_store: Option<PathBuf>,
    /// 宿主接受作者声明线放行不可逆动作（步 20j-2，B128；`--release-on-declared`）：进 `entry_hash`，报告 `accept`
    pub release_on_declared: bool,
    /// 宿主开启放行把关（意图汇编 11a，`--guard`）：进 `Program.entry.guard`、`entry_hash`，报告 `guard`
    pub guard: bool,
    /// 生成器模型（步 15h-1，B149）：给了就把 `gen` 实例换成生成器端口（`claude -p --model <m>`）
    pub gen_model: Option<String>,
    /// 伴随题的发法（B0492 S5；`--companions same|parallel|off`）：不给用 `Passes` 的默认值（等 H2 实验定）
    pub companions: Option<jpp::interp::CompanionMode>,
    /// 单元图开关（C2b，步 41；`--cells on|off`，H1a 对照臂）：不给即开
    pub cells: Option<bool>,
    /// 报告带单元图统计（C2c，`--cells-stats`）：不给时报告逐字节不变
    pub cells_stats: bool,
    /// 生成器画像文件（B149；没给时按 `--profiles-dir` 或可执行文件旁 `profiles/` 找）
    pub gen_profile: Option<PathBuf>,
    /// 跨运行缓存目录（步 19，B151 两段式；取代 15h-2 的 `--gen-cache`）：目录里的账本建索引，
    /// 判断、生成、变换按不含调用位置的缓存键命中即不再调用
    pub cache: Option<PathBuf>,
    /// 调用者的追踪上下文（C-2，`--trace-parent`，W3C `traceparent` 文本）：被调用段由它推导
    /// （[`jpp::ledger::TraceCtx::enter`]，标签见 `trace_label`）。与 `trace_seed` 互斥；都不给由会话按账本推导
    pub trace_parent: Option<String>,
    /// 链的起点的种子（C-2，`--trace-seed`）：追踪编号由它推导，本段是根段
    pub trace_seed: Option<String>,
    /// 推导本段的标签（C-2，`--trace-label`）：缺省是程序标识。同一父段下同一程序同一标签是同一段；
    /// 要在树上分成两个节点，换标签
    pub trace_label: Option<String>,
    /// 费用确认（Z0236，`11` §5.5）：给了就允许上界超阈值的运行开跑；只在这一趟会核阈值时有意义
    pub confirm: bool,
    /// 确认阈值（美元；`--confirm-above`），没给取 [`CONFIRM_DEFAULT_USD`]
    pub confirm_above: Option<f64>,
    /// 上游余额文件（C-3）：`CarryRecord` 的 JSON；给了就把本轮预算收成 min(声明, 余额)、深度接着算
    pub carry_in: Option<PathBuf>,
    /// 把本轮交给下一轮的余额写成同一形状的文件（C-3；要求 `--carry-in`）
    pub carry_out: Option<PathBuf>,
    /// 显式重新授权（C-3 R1，`--carry-reauthorize`）：续跑同一轮时 `--carry-in` 的余额从这一趟起开新段
    pub carry_reauthorize: bool,
}

/// 费用确认的默认阈值（美元），`11` §5.5：「超阈值（默认 $0.10）要 `--confirm`」。它是操作者对自己花费的容忍度，
/// 不是判断器的属性，所以不进画像；宿主 crate 里的具名常量，内核不认识这个数（`scripts/grep_constants.py`）。
pub const CONFIRM_DEFAULT_USD: f64 = 0.10;

/// 取出 `--json`（步 9a）：只有 `check` 与 `run` 收它，出现一次；其余参数原样交给 [`parse`]。
pub fn take_json(args: &mut Vec<String>) -> Result<bool, String> {
    let n = args.iter().skip(1).filter(|a| *a == "--json").count();
    if n == 0 {
        return Ok(false);
    }
    if !matches!(
        args.first().map(String::as_str),
        Some("check" | "run" | "bank-stats" | "ledger-tree")
    ) {
        return Err(
            "--json is accepted only by check and run (and bank-stats, ledger-tree)".into(),
        );
    }
    if n > 1 {
        return Err("--json was supplied twice".into());
    }
    let i = args.iter().skip(1).position(|a| a == "--json").unwrap() + 1;
    args.remove(i);
    Ok(true)
}

/// 宿主入口的目的、材料条目与料库（B0472，推进主会话 B0672「线 A 全程只经 CLI」）：`--purpose <text>`、
/// `--mat <name>=<file>`（可重复）、`--mat-store <dir>`（只 `run`）。与 `--json` 同法在解析前取出，`Command` 的形状不变
#[derive(Debug, Default, PartialEq)]
pub struct EntryFlags {
    pub purpose: Option<String>,
    /// 材料条目：名字与文件（`.json` 按 JSON 读，其余按 UTF-8 文本读）
    pub mats: Vec<(String, PathBuf)>,
    pub mat_store: Option<PathBuf>,
}

/// 入口条目名：字母（含中文）或下划线开头，其余字母、数字、下划线
fn 是标识符(n: &str) -> bool {
    let mut cs = n.chars();
    cs.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && cs.all(|c| c.is_alphanumeric() || c == '_')
}

/// 取出 [`EntryFlags`]：只有 `check` 与 `run` 收；`--purpose`、`--mat-store` 至多一次；`--mat` 的名字是标识符、
/// 不重名、不用 `--input` 与 `--purpose` 占的 `input`、`purpose`（不论那两个开关给没给）
pub fn take_entry(args: &mut Vec<String>) -> Result<EntryFlags, String> {
    let verb = args.first().cloned().unwrap_or_default();
    let mut f = EntryFlags::default();
    let mut i = 1;
    while i < args.len() {
        let flag = args[i].clone();
        if !matches!(flag.as_str(), "--purpose" | "--mat" | "--mat-store") {
            i += 1;
            continue;
        }
        if !matches!(verb.as_str(), "check" | "run") {
            return Err(format!("{flag} is accepted only by check and run"));
        }
        if flag == "--mat-store" && verb != "run" {
            return Err("--mat-store is accepted only by run".into());
        }
        let what = match flag.as_str() {
            "--purpose" => "<text>",
            "--mat" => "<name>=<file>",
            _ => "<dir>",
        };
        let v = args
            .get(i + 1)
            .filter(|s| !s.starts_with("--"))
            .cloned()
            .ok_or_else(|| format!("{flag} requires a value {what}"))?;
        match flag.as_str() {
            "--purpose" => {
                if f.purpose.is_some() {
                    return Err("--purpose was supplied twice".into());
                }
                f.purpose = Some(v);
            }
            "--mat" => {
                let (name, file) = v
                    .split_once('=')
                    .filter(|(n, p)| !n.is_empty() && !p.is_empty())
                    .ok_or_else(|| format!("--mat expects <name>=<file>, got '{v}'"))?;
                if !是标识符(name) {
                    return Err(format!("--mat name '{name}' is not an identifier"));
                }
                if matches!(name, "input" | "purpose") {
                    return Err(format!(
                        "--mat name '{name}' is reserved (--{name} binds it)"
                    ));
                }
                if f.mats.iter().any(|(n, _)| n == name) {
                    return Err(format!("--mat name '{name}' was supplied twice"));
                }
                f.mats.push((name.to_string(), PathBuf::from(file)));
            }
            _ => {
                if f.mat_store.is_some() {
                    return Err("--mat-store was supplied twice".into());
                }
                f.mat_store = Some(PathBuf::from(v));
            }
        }
        args.drain(i..=i + 1);
    }
    Ok(f)
}

/// `--explain`（Z0190 后一半，`20` §2.3 `explain`）：`check` 与 `run` 在动手前打印计划（估计、预算比对、站点、
/// 未估）。与 `--json` 同法在解析前取出，`Command` 不改；只有 `check` 与 `run` 收，出现一次。
pub fn take_explain(args: &mut Vec<String>) -> Result<bool, String> {
    let n = args.iter().skip(1).filter(|a| *a == "--explain").count();
    if n == 0 {
        return Ok(false);
    }
    if !matches!(args.first().map(String::as_str), Some("check" | "run")) {
        return Err("--explain is accepted only by check and run".into());
    }
    if n > 1 {
        return Err("--explain was supplied twice".into());
    }
    let i = args.iter().skip(1).position(|a| a == "--explain").unwrap() + 1;
    args.remove(i);
    Ok(true)
}

/// `check --questions-out <file.json>`（步 20a-2b，B116 (5)）：导出程序里的字面题，供校准键迁移
/// （20a-2e，`jpp calib-migrate --questions`）算零槽题式哈希。与 `--json` 同法在解析前取出，
/// `check` 其余参数的解析与报文不变；只有 `check` 收它。
pub fn take_questions_out(args: &mut Vec<String>) -> Result<Option<PathBuf>, String> {
    let n = args
        .iter()
        .skip(1)
        .filter(|a| *a == "--questions-out")
        .count();
    if n == 0 {
        return Ok(None);
    }
    if args.first().map(String::as_str) != Some("check") {
        return Err("--questions-out is accepted only by check".into());
    }
    if n > 1 {
        return Err("--questions-out was supplied twice".into());
    }
    let i = args
        .iter()
        .skip(1)
        .position(|a| a == "--questions-out")
        .unwrap()
        + 1;
    let path = args
        .get(i + 1)
        .filter(|p| !p.starts_with("--"))
        .cloned()
        .ok_or("--questions-out requires a value <file.json>")?;
    args.drain(i..=i + 1);
    Ok(Some(PathBuf::from(path)))
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        return Ok(Command::Help);
    }
    let verb = args[0].as_str();
    if verb == "calib-import" {
        return parse_import(args);
    }
    if verb == "bank" || verb == "bank-stats" {
        return Ok(Command::Bank {
            stats: verb == "bank-stats",
            args: args[1..].to_vec(),
        });
    }
    if verb == "derive-admit" {
        return Ok(Command::DeriveAdmit {
            args: args[1..].to_vec(),
        });
    }
    if verb == "profile" {
        let (Some(sub), Some(file), None) = (args.get(1), args.get(2), args.get(3)) else {
            return Err("profile requires check <profile.json>".into());
        };
        if sub != "check" {
            return Err(format!(
                "unknown profile subcommand {sub:?}; profile requires check <profile.json>"
            ));
        }
        return Ok(Command::ProfileCheck {
            profile: file.into(),
        });
    }
    if verb == "ledger-migrate" {
        let (Some(from), Some(to), None) = (args.get(1), args.get(2), args.get(3)) else {
            return Err("ledger-migrate requires <v2-ledger> <v3-out>".into());
        };
        return Ok(Command::LedgerMigrate {
            from: from.into(),
            to: to.into(),
        });
    }
    if verb == "ledger-tree" {
        let ledgers: Vec<PathBuf> = args[1..].iter().map(PathBuf::from).collect();
        if ledgers.is_empty() || args[1..].iter().any(|a| a.starts_with("--")) {
            return Err(
                "ledger-tree requires one or more <ledger> files (and optionally --json)".into(),
            );
        }
        return Ok(Command::LedgerTree { ledgers });
    }
    if verb == "calib-confirm" {
        let dir = args
            .get(1)
            .ok_or("calib-confirm requires <calib-dir> <key> --suspend|--keep")?;
        let key = args
            .get(2)
            .ok_or("calib-confirm requires <calib-dir> <key> --suspend|--keep")?;
        let suspend = match args.get(3).map(String::as_str) {
            Some("--suspend") => true,
            Some("--keep") => false,
            _ => return Err("calib-confirm requires --suspend or --keep".into()),
        };
        return Ok(Command::CalibConfirm {
            dir: dir.into(),
            key: key.clone(),
            suspend,
        });
    }
    if !matches!(verb, "parse" | "check" | "run") {
        return Err(format!("unknown command '{verb}'"));
    }
    let source = args
        .get(1)
        .filter(|s| !s.starts_with("--"))
        .ok_or_else(|| format!("{verb} requires a .jpp source file"))?;
    if verb == "parse" {
        return match &args[2..] {
            [] => Ok(Command::Parse {
                source: source.into(),
                ast: false,
            }),
            [flag] if flag == "--ast" => Ok(Command::Parse {
                source: source.into(),
                ast: true,
            }),
            _ => Err("parse accepts only --ast after the source file".into()),
        };
    }
    if verb == "check" {
        // 步 20j-2（B128）：`--release-on-declared` 可在任意位置，先取出，余下的按原有形状匹配；
        // `--guard`（意图汇编 11a）同法
        let 次数 = |flag: &str| args[2..].iter().filter(|a| *a == flag).count();
        for flag in ["--release-on-declared", "--guard"] {
            if 次数(flag) > 1 {
                return Err(format!("{flag} was supplied twice"));
            }
        }
        let mut rest: Vec<String> = args[2..]
            .iter()
            .filter(|a| *a != "--release-on-declared" && *a != "--guard")
            .cloned()
            .collect();
        let release_on_declared = 次数("--release-on-declared") == 1;
        let guard = 次数("--guard") == 1;
        // `--calib <dir>`、`--profile <file>`（L7 2026-09-28，K-084/K-088；B32 时延静态面要画像 p95）：
        // 同法先取出，余下的按原有形状匹配
        let mut 取值 = |flag: &str, what: &str| -> Result<Option<PathBuf>, String> {
            if rest.iter().filter(|a| *a == flag).count() > 1 {
                return Err(format!("{flag} was supplied twice"));
            }
            let Some(i) = rest.iter().position(|a| a == flag) else {
                return Ok(None);
            };
            let v = rest
                .get(i + 1)
                .filter(|d| !d.starts_with("--"))
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value <{what}>"))?;
            rest.drain(i..=i + 1);
            Ok(Some(PathBuf::from(v)))
        };
        let calib = 取值("--calib", "calib-dir")?;
        let profile = 取值("--profile", "profile.json")?;
        return match rest.as_slice() {
            [] => Ok(Command::Check {
                source: source.into(),
                input: None,
                input_trusted: false,
                release_on_declared,
                guard,
                calib: calib.clone(),
                profile: profile.clone(),
            }),
            [flag, path] if flag == "--input" && !path.starts_with("--") => Ok(Command::Check {
                source: source.into(),
                input: Some(path.into()),
                input_trusted: false,
                release_on_declared,
                guard,
                calib: calib.clone(),
                profile: profile.clone(),
            }),
            [flag, path, trusted]
                if flag == "--input" && !path.starts_with("--") && trusted == "--input-trusted" =>
            {
                Ok(Command::Check {
                    source: source.into(),
                    input: Some(path.into()),
                    input_trusted: true,
                    release_on_declared,
                    guard,
                    calib: calib.clone(),
                    profile: profile.clone(),
                })
            }
            [trusted] if trusted == "--input-trusted" => {
                Err("check --input-trusted requires --input <file.json>".into())
            }
            _ => Err(
                "check accepts only a source file, --input <file.json>, --input-trusted, --release-on-declared, --guard, --calib <calib-dir> and --profile <profile.json>".into(),
            ),
        };
    }
    let mut options = RunOptions {
        source: source.into(),
        fixtures: None,
        output: None,
        ledger_out: None,
        replay: None,
        resume: None,
        profile: None,
        calib: None,
        calib_out: None,
        backend: Backend::Fixed,
        model: None,
        profiles_dir: None,
        input: None,
        input_trusted: false,
        mat_store: None,
        release_on_declared: false,
        guard: false,
        gen_model: None,
        companions: None,
        cells: None,
        cells_stats: false,
        gen_profile: None,
        cache: None,
        trace_parent: None,
        trace_seed: None,
        trace_label: None,
        confirm: false,
        confirm_above: None,
        carry_in: None,
        carry_out: None,
        carry_reauthorize: false,
    };
    let mut backend_set = false;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--backend" => {
                if backend_set {
                    return Err("--backend was supplied twice".into());
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| {
                        format!(
                            "--backend requires a value (fixed|{})",
                            jpp::backends::names()
                        )
                    })?;
                options.backend = match value.as_str() {
                    "fixed" => Backend::Fixed,
                    other => match jpp::backends::by_name(other) {
                        Some(s) => Backend::Registered(s),
                        None => {
                            return Err(format!(
                                "unknown --backend '{other}' (expected fixed|{})",
                                jpp::backends::names()
                            ));
                        }
                    },
                };
                backend_set = true;
                i += 2;
            }
            "--model" => {
                if options.model.is_some() {
                    return Err("--model was supplied twice".into());
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| "--model requires a value".to_string())?;
                options.model = Some(value.clone());
                i += 2;
            }
            "--companions" => {
                if options.companions.is_some() {
                    return Err("--companions was supplied twice".into());
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| {
                        "--companions requires a value (same|parallel|off)".to_string()
                    })?;
                options.companions = Some(match value.as_str() {
                    "same" => jpp::interp::CompanionMode::Same,
                    "parallel" => jpp::interp::CompanionMode::Parallel,
                    "off" => jpp::interp::CompanionMode::Off,
                    other => {
                        return Err(format!(
                            "unknown --companions '{other}' (expected same|parallel|off)"
                        ));
                    }
                });
                i += 2;
            }
            "--cells-stats" => {
                options.cells_stats = true;
                i += 1;
            }
            "--cells" => {
                if options.cells.is_some() {
                    return Err("--cells was supplied twice".into());
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| "--cells requires a value (on|off)".to_string())?;
                options.cells = Some(match value.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("unknown --cells '{other}' (expected on|off)")),
                });
                i += 2;
            }
            "--gen-model" => {
                if options.gen_model.is_some() {
                    return Err("--gen-model was supplied twice".into());
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| "--gen-model requires a value (e.g. sonnet)".to_string())?;
                options.gen_model = Some(value.clone());
                i += 2;
            }
            // C-2：追踪上下文。`--trace-parent` 是调用者的 traceparent，`--trace-seed` 是链的起点的种子
            "--trace-parent" | "--trace-seed" | "--trace-label" => {
                let target = match args[i].as_str() {
                    "--trace-parent" => &mut options.trace_parent,
                    "--trace-seed" => &mut options.trace_seed,
                    _ => &mut options.trace_label,
                };
                if target.is_some() {
                    return Err(format!("{} was supplied twice", args[i]));
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| format!("{} requires a value", args[i]))?;
                *target = Some(value.clone());
                i += 2;
            }
            "--input-trusted" => {
                if options.input_trusted {
                    return Err("--input-trusted was supplied twice".into());
                }
                options.input_trusted = true;
                i += 1;
            }
            // 步 20j-2（B128）：宿主接受作者声明线放行；不要求 `--input`
            "--release-on-declared" => {
                if options.release_on_declared {
                    return Err("--release-on-declared was supplied twice".into());
                }
                options.release_on_declared = true;
                i += 1;
            }
            // Z0236（`11` §5.5）：操作者确认这一趟的花费
            "--confirm" => {
                if options.confirm {
                    return Err("--confirm was supplied twice".into());
                }
                options.confirm = true;
                i += 1;
            }
            "--confirm-above" => {
                if options.confirm_above.is_some() {
                    return Err("--confirm-above was supplied twice".into());
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| {
                        "--confirm-above requires a value in USD (e.g. 0.05)".to_string()
                    })?;
                let usd: f64 = value.parse().map_err(|_| {
                    format!("--confirm-above expects a number of USD, got '{value}'")
                })?;
                if !usd.is_finite() || usd < 0.0 {
                    return Err(format!(
                        "--confirm-above expects a finite, non-negative number of USD, got '{value}'"
                    ));
                }
                options.confirm_above = Some(usd);
                i += 2;
            }
            // 意图汇编 11a：宿主开启放行把关
            // C-3 R1：显式重新授权
            "--carry-reauthorize" => {
                if options.carry_reauthorize {
                    return Err("--carry-reauthorize was supplied twice".into());
                }
                options.carry_reauthorize = true;
                i += 1;
            }
            "--guard" => {
                if options.guard {
                    return Err("--guard was supplied twice".into());
                }
                options.guard = true;
                i += 1;
            }
            _ => {
                let target = match args[i].as_str() {
                    "--fixtures" => &mut options.fixtures,
                    "--output" => &mut options.output,
                    "--ledger-out" => &mut options.ledger_out,
                    "--replay" => &mut options.replay,
                    "--resume" => &mut options.resume,
                    "--profile" => &mut options.profile,
                    "--calib" => &mut options.calib,
                    "--calib-out" => &mut options.calib_out,
                    "--profiles-dir" => &mut options.profiles_dir,
                    "--input" => &mut options.input,
                    "--gen-profile" => &mut options.gen_profile,
                    "--cache" => &mut options.cache,
                    // C-3：上游余额进出
                    "--carry-in" => &mut options.carry_in,
                    "--carry-out" => &mut options.carry_out,
                    other => return Err(format!("unknown run option '{other}'")),
                };
                if target.is_some() {
                    return Err(format!("{} was supplied twice", args[i]));
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| format!("{} requires a file path", args[i]))?;
                *target = Some(value.into());
                i += 2;
            }
        }
    }
    if options.input_trusted && options.input.is_none() {
        return Err("run --input-trusted requires --input <file.json>".into());
    }
    if options.trace_parent.is_some() && options.trace_seed.is_some() {
        return Err(
            "use either --trace-parent (a caller's context) or --trace-seed (start a chain)".into(),
        );
    }
    if options.trace_label.is_some()
        && options.trace_parent.is_none()
        && options.trace_seed.is_none()
    {
        return Err("--trace-label requires --trace-parent or --trace-seed".into());
    }
    if let Some(tp) = &options.trace_parent {
        jpp::ledger::TraceCtx::from_traceparent(tp)?;
    }
    // C-3：只凭账本重放从账本头取余额，不收宿主给的；交回余额要有进门余额
    if options.carry_in.is_some() && options.replay.is_some() {
        return Err(
            "--carry-in cannot be combined with --replay: replay takes the carried balance from the ledger header"
                .into(),
        );
    }
    if options.carry_reauthorize && options.carry_in.is_none() {
        return Err("run --carry-reauthorize requires --carry-in <file.json>".into());
    }
    if options.carry_out.is_some() && options.carry_in.is_none() && options.replay.is_none() {
        return Err("run --carry-out requires --carry-in <file.json> (or --replay, which takes the balance from the ledger header)".into());
    }
    if options.replay.is_some() && options.resume.is_some() {
        return Err(
            "use either --replay (no new requests) or --resume (continue with the client)".into(),
        );
    }
    if let (Some(_), Some(s)) = (&options.fixtures, options.backend.spec()) {
        return Err(format!(
            "--fixtures and --backend {} are mutually exclusive",
            s.name
        ));
    }
    // B127 过渡守卫（20a-2 合入前）：校准记录没有模型分量，只有 fixed 与 live 能用现有校准记录；
    // 其他后端读（`--calib`）或写（`--calib-out`，主会话 2026-09-25 决定一并守住）都会跨判断器借线。
    // 依据：B127（地基/附注/2026-09-25-待补批量裁定-2.md §七）
    if let Some(s) = options.backend.spec().filter(|s| !s.calib)
        && (options.calib.is_some() || options.calib_out.is_some())
    {
        return Err(format!(
            "E-calib-model: --backend {} 不能带 --calib 或 --calib-out：校准记录尚无模型分量（B60，20a-2），不得跨判断器借线",
            s.name
        ));
    }
    if options.model.is_some() && options.backend == Backend::Fixed {
        return Err(format!(
            "--model requires --backend {}",
            jpp::backends::names()
        ));
    }
    if options.profiles_dir.is_some() && options.backend == Backend::Fixed {
        return Err(format!(
            "--profiles-dir requires --backend {}",
            jpp::backends::names()
        ));
    }
    check_gen(&options)?;
    Ok(Command::Run(options))
}

/// `--gen-model` / `--gen-profile` 的组合规则（步 15h-1，B149）：生成器端口只在注册后端的首跑与续接里
/// 替换 `gen` 实例；夹具自带生成、重放不调用；启用开关与 `--backend live` 同在 feature `live` 下。
fn check_gen(options: &RunOptions) -> Result<(), String> {
    if options.gen_profile.is_some() && options.gen_model.is_none() {
        return Err("--gen-profile requires --gen-model".into());
    }
    if options.gen_model.is_none() {
        return Ok(());
    }
    if options.fixtures.is_some() {
        return Err(
            "--gen-model and --fixtures are mutually exclusive (fixtures supply generations)"
                .into(),
        );
    }
    if options.replay.is_some() {
        return Err(
            "--gen-model and --replay are mutually exclusive (replay makes no requests)".into(),
        );
    }
    if options.backend == Backend::Fixed {
        return Err(format!(
            "--gen-model requires --backend {}",
            jpp::backends::names()
        ));
    }
    if !cfg!(feature = "live") {
        return Err("--gen-model requires jpp built with --features live".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 比赛块 C-1、C-1b（R）：帮助文本的动作句与改前手写的原文逐字相同，模板占位已填；
    /// R2b 起表里每加一行动作，这里的期望按表序追加。
    #[test]
    fn 帮助文本的动作清单与原文相同() {
        assert_eq!(
            jpp::actions::usage_list(),
            "record_check, read_json(path), write_json(path,value), graph:matching(graph), graph:shortest_path(graph), graph:max_clique(graph), graph:components(graph), graph:set_cover(graph), graph:max_flow(graph), graph:cycles(graph), exec_py(code,stdin,timeout_s), check_tests(code,tests,timeout_s), embed_topk(texts,query,k), bm25_topk(query,corpus,k), exec_sql(db,sql)"
        );
        let h = help();
        assert_eq!(
            h.matches("Registered actions: record_check, read_json(path), write_json(path,value), graph:matching(graph), graph:shortest_path(graph), graph:max_clique(graph), graph:components(graph), graph:set_cover(graph), graph:max_flow(graph), graph:cycles(graph), exec_py(code,stdin,timeout_s), check_tests(code,tests,timeout_s), embed_topk(texts,query,k), bm25_topk(query,corpus,k), exec_sql(db,sql).")
                .count(),
            1
        );
        assert!(!h.contains("{actions}"));
    }
    fn args(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn accepts_combined_fixture_replay_and_report_paths() {
        let Command::Run(run) = parse(&args(&[
            "run",
            "a file.jpp",
            "--fixtures",
            "fixture.json",
            "--replay",
            "ledger.json",
            "--output",
            "report.json",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert_eq!(run.source, PathBuf::from("a file.jpp"));
        assert_eq!(run.replay, Some("ledger.json".into()));
        assert_eq!(run.output, Some("report.json".into()));
    }

    #[test]
    fn reports_usage_mistakes_before_execution() {
        for argv in [
            vec!["run"],
            vec!["run", "a.jpp", "--output"],
            vec!["run", "a.jpp", "--output", "--replay", "x"],
            vec!["run", "a.jpp", "--fixtures", "a", "--fixtures", "b"],
            vec!["check", "a.jpp", "--ast"],
            vec!["run", "a.jpp", "--resume", "a", "--replay", "b"],
            vec!["run", "a.jpp", "--backend", "bogus"],
            vec!["run", "a.jpp", "--backend", "live", "--fixtures", "f.json"],
            vec!["run", "a.jpp", "--model", "jev-1.13.0"],
            vec!["run", "a.jpp", "--backend", "live", "--backend", "fixed"],
            vec!["run", "a.jpp", "--profiles-dir", "profiles"],
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }

    #[test]
    fn backend_defaults_to_fixed_and_live_needs_explicit_opt_in() {
        let Command::Run(run) = parse(&args(&["run", "a.jpp"])).unwrap() else {
            panic!("run")
        };
        assert_eq!(run.backend, Backend::Fixed);
        assert_eq!(run.model, None);
    }

    #[test]
    fn backend_live_accepts_a_model_name() {
        let Command::Run(run) = parse(&args(&[
            "run",
            "a.jpp",
            "--backend",
            "live",
            "--model",
            "jev-1.13.0",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert_eq!(run.backend, Backend::Registered(&jpp::backends::jev::SPEC));
        assert_eq!(run.model, Some("jev-1.13.0".into()));
    }

    #[test]
    fn run_and_check_accept_an_input_file() {
        let Command::Run(run) = parse(&args(&["run", "a.jpp", "--input", "m.json"])).unwrap()
        else {
            panic!("run")
        };
        assert_eq!(run.input, Some("m.json".into()));
        assert_eq!(
            parse(&args(&["check", "a.jpp", "--input", "m.json"])).unwrap(),
            Command::Check {
                source: "a.jpp".into(),
                input: Some("m.json".into()),
                input_trusted: false,
                release_on_declared: false,
                guard: false,
                calib: None,
                profile: None,
            }
        );
        for argv in [
            vec!["run", "a.jpp", "--input", "a.json", "--input", "b.json"],
            vec!["run", "a.jpp", "--input"],
            vec!["check", "a.jpp", "--input"],
            vec!["check", "a.jpp", "--fixtures", "f.json"],
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }

    /// 步 15h-1 (h)：`--gen-model` 的组合规则。
    #[test]
    fn gen_model_组合规则() {
        // 步 15h-1 (h)：夹具自带生成、重放不调用、固定观察没有注册后端；非 live 构建不收
        let err = |xs: &[&str]| parse(&args(xs)).unwrap_err();
        assert!(
            err(&[
                "run",
                "a.jpp",
                "--backend",
                "live",
                "--gen-model",
                "sonnet",
                "--fixtures",
                "f.json"
            ])
            .contains("--fixtures")
        );
        assert!(
            err(&[
                "run",
                "a.jpp",
                "--backend",
                "live",
                "--gen-model",
                "sonnet",
                "--replay",
                "l.json"
            ])
            .contains("--replay")
        );
        assert!(err(&["run", "a.jpp", "--gen-model", "sonnet"]).contains("--backend"));
        assert!(err(&["run", "a.jpp", "--gen-profile", "g.json"]).contains("--gen-model"));
        let live = parse(&args(&[
            "run",
            "a.jpp",
            "--backend",
            "live",
            "--gen-model",
            "sonnet",
        ]));
        if cfg!(feature = "live") {
            let Ok(Command::Run(run)) = live else {
                panic!("live 构建应当接受 --gen-model")
            };
            assert_eq!(run.gen_model.as_deref(), Some("sonnet"));
        } else {
            assert!(live.unwrap_err().contains("--features live"));
        }
    }

    /// 步 14b-1（B108）：`--input-trusted` 在 `run`/`check` 都收，且都要求先有 `--input`。
    #[test]
    fn input_trusted_requires_input() {
        let Command::Run(run) = parse(&args(&[
            "run",
            "a.jpp",
            "--input",
            "m.json",
            "--input-trusted",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert!(run.input_trusted);
        assert_eq!(
            parse(&args(&[
                "check",
                "a.jpp",
                "--input",
                "m.json",
                "--input-trusted"
            ]))
            .unwrap(),
            Command::Check {
                source: "a.jpp".into(),
                input: Some("m.json".into()),
                input_trusted: true,
                release_on_declared: false,
                guard: false,
                calib: None,
                profile: None,
            }
        );
        for argv in [
            vec!["run", "a.jpp", "--input-trusted"],
            vec!["check", "a.jpp", "--input-trusted"],
            vec![
                "run",
                "a.jpp",
                "--input",
                "m.json",
                "--input-trusted",
                "--input-trusted",
            ],
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }

    /// 步 20j-2（B128）：`--release-on-declared` 在 `run`/`check` 都收、不要求 `--input`、位置不限，重复报错
    #[test]
    fn release_on_declared_is_accepted_by_run_and_check() {
        let Command::Run(run) = parse(&args(&["run", "a.jpp", "--release-on-declared"])).unwrap()
        else {
            panic!("run")
        };
        assert!(run.release_on_declared && run.input.is_none());
        let Command::Run(run) = parse(&args(&["run", "a.jpp"])).unwrap() else {
            panic!("run")
        };
        assert!(!run.release_on_declared);
        assert_eq!(
            parse(&args(&["check", "a.jpp", "--release-on-declared"])).unwrap(),
            Command::Check {
                source: "a.jpp".into(),
                input: None,
                input_trusted: false,
                release_on_declared: true,
                guard: false,
                calib: None,
                profile: None,
            }
        );
        assert_eq!(
            parse(&args(&[
                "check",
                "a.jpp",
                "--release-on-declared",
                "--input",
                "m.json",
                "--input-trusted"
            ]))
            .unwrap(),
            Command::Check {
                source: "a.jpp".into(),
                input: Some("m.json".into()),
                input_trusted: true,
                release_on_declared: true,
                guard: false,
                calib: None,
                profile: None,
            }
        );
        for argv in [
            vec![
                "run",
                "a.jpp",
                "--release-on-declared",
                "--release-on-declared",
            ],
            vec![
                "check",
                "a.jpp",
                "--release-on-declared",
                "--release-on-declared",
            ],
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }

    /// 意图汇编 11a：`--guard` 在 `run`/`check` 都收、位置不限、重复报错；缺省关
    #[test]
    fn guard_is_accepted_by_run_and_check() {
        let Command::Run(run) = parse(&args(&["run", "a.jpp", "--guard"])).unwrap() else {
            panic!("run")
        };
        assert!(run.guard && !run.release_on_declared);
        let Command::Run(run) = parse(&args(&["run", "a.jpp"])).unwrap() else {
            panic!("run")
        };
        assert!(!run.guard);
        assert_eq!(
            parse(&args(&[
                "check",
                "a.jpp",
                "--guard",
                "--release-on-declared"
            ]))
            .unwrap(),
            Command::Check {
                source: "a.jpp".into(),
                input: None,
                input_trusted: false,
                release_on_declared: true,
                guard: true,
                calib: None,
                profile: None,
            }
        );
        for argv in [
            vec!["run", "a.jpp", "--guard", "--guard"],
            vec!["check", "a.jpp", "--guard", "--guard"],
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }

    #[test]
    fn backend_live_accepts_a_profiles_dir() {
        let Command::Run(run) = parse(&args(&[
            "run",
            "a.jpp",
            "--backend",
            "live",
            "--profiles-dir",
            "p",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert_eq!(run.profiles_dir, Some("p".into()));
    }

    /// Z0236（`11` §5.5）：`--confirm` 与 `--confirm-above <美元>` 只给 `run`；默认不确认、阈值取默认
    #[test]
    fn confirm_options_parse_on_run() {
        let Command::Run(run) = parse(&args(&["run", "a.jpp"])).unwrap() else {
            panic!("run")
        };
        assert!(!run.confirm);
        assert_eq!(run.confirm_above, None);
        let Command::Run(run) = parse(&args(&[
            "run",
            "a.jpp",
            "--confirm",
            "--confirm-above",
            "0.05",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert!(run.confirm);
        assert_eq!(run.confirm_above, Some(0.05));
        let Command::Run(run) = parse(&args(&["run", "a.jpp", "--confirm-above", "0"])).unwrap()
        else {
            panic!("run")
        };
        assert_eq!(run.confirm_above, Some(0.0));
    }

    #[test]
    fn confirm_options_reject_bad_values_and_other_verbs() {
        for argv in [
            vec!["run", "a.jpp", "--confirm", "--confirm"],
            vec!["run", "a.jpp", "--confirm-above"],
            vec!["run", "a.jpp", "--confirm-above", "--confirm"],
            vec!["run", "a.jpp", "--confirm-above", "abc"],
            vec!["run", "a.jpp", "--confirm-above", "-1"],
            vec!["run", "a.jpp", "--confirm-above", "nan"],
            vec!["run", "a.jpp", "--confirm-above", "inf"],
            vec![
                "run",
                "a.jpp",
                "--confirm-above",
                "0.1",
                "--confirm-above",
                "0.2",
            ],
            vec!["check", "a.jpp", "--confirm"],
            vec!["check", "a.jpp", "--confirm-above", "0.1"],
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }

    #[test]
    fn help_names_the_confirm_options_and_fills_the_default() {
        let h = help();
        assert!(h.contains("[--confirm] [--confirm-above <usd>]"));
        assert!(h.contains("default 0.10 USD"));
        assert!(!h.contains("{CONFIRM_DEFAULT}"));
    }
}
