# -*- coding: utf-8 -*-
"""**限定条件的留存率**：一个数被写出 N 次，它的限定跟着走了几次。只读。

起因：「校准集是两模型都同意的子集」这件事该长在数据、代码还是流程上。
量一下就知道这个三分法不是对的轴——**这个限定今天已经横跨三者写在六处，
而它照样被丢掉一半**。
"""
import pathlib
ROOT = pathlib.Path(__file__).resolve().parents[3]
FILES = ["DECISIONS.md", "foundation/experiments/前提结论.md", "foundation/experiments/EXPERIMENTS.md",
         "自检-当前状态.md", "09-研究方法与假设账本.md", "待Nature裁定清单.md",
         "foundation/experiments/e_cal_曲线.md", "设计/保形弃权域-设计-2026-09-21.md"]

def scan(num, quals, label, rad=3):
    tot = car = 0; miss = []
    for f in FILES:
        p = ROOT / f
        if not p.exists(): continue
        L = p.read_text(encoding="utf-8").split("\n")
        for i, l in enumerate(L):
            if num not in l: continue
            tot += 1
            if any(q in L[j] for q in quals for j in range(max(0, i-rad), min(len(L), i+rad+1))): car += 1
            else: miss.append(f"{f}:{i+1}")
    print(f"  {label:44s} {tot:3d} 次，带限定 {car:3d} = {car/tot:.0%}" if tot else f"  {label}: 未出现")
    return miss

print("『写在旁边』的限定，留存率（同段 ±3 行内出现即算带）：")
m1 = scan("0.057", ["模型双标"], "ECE 0.057 + 「真值是模型双标子集」")
m2 = scan("0.748", ["模型双标"], "AUC 0.748 + 「真值是模型双标子集」")
scan("置换一致率", ["恒真"], "choice 置换一致率 + 「恒真项」")
print("\n**没有一条守住 100%。** 而这个限定今天已经写在六处：")
print("  1 readings.jsonl 每一行的 label_source   2 校准记录 set_id 串里的「模型双标」四个字")
print("  3 校准记录 source 字段的整段散文          4 档案 calibration.zh_reliability_curve.label_source")
print("  5 前提结论.md 表格上面一行                6 前提结论.md 的专节（总控 2026-09-21 加）")
print("\n最贵的两处丢失——那个数成了另一个实验预注册里的赌的前提：")
for x in m1:
    if "EXPERIMENTS" in x: print(f"  {x}")
