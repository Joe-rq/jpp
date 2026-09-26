#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""建 examples/fixtures/returns.db（意图汇编 7a 用的样例库：代码能定的用代码判，JEV 只判语义）。

一张退货表 returns(id, category, product_name, quantity, return_date)，八行数据，覆盖 2026 年
Q1-Q3。数据故意设计成：不限定季度、限定季度但按季度边界差一天、限定季度但按单个商品分组，
这三种写法都能跑出非空结果、但都答错「上个季度（以 2026 年第三季度为今天，上个季度是 2026 年
4-6 月）哪一类商品退货最多」这个问题；只有同时限定 2026-04-01 到 2026-07-01（不含）且按
category 分组的查询才对（数码，40 件）。这张库只用于 examples/search-ground.jpp 与
examples/guide/comp-ground-verify.jpp，运行前先跑本脚本重建：

    cd 地基/rust-jpp
    python3 examples/fixtures/build-returns-db.py

会覆盖已存在的 returns.db（仓库里也直接带着建好的这一份，跑脚本只是为了可重现）。
"""
import os
import sqlite3

HERE = os.path.dirname(os.path.abspath(__file__))
DB = os.path.join(HERE, "returns.db")

ROWS = [
    # id, category, product_name, quantity, return_date
    (1, "家居", "收纳盒", 50, "2026-02-10"),  # Q1：全年单笔最大，制造「不限定季度」的错误答案（家居）
    (2, "数码", "耳机", 25, "2026-05-05"),  # Q2，正确答案的一部分
    (3, "数码", "充电器", 15, "2026-06-30"),  # Q2 最后一天：制造「季度边界差一天」的错误答案
    (4, "服装", "衬衫", 10, "2026-04-15"),  # Q2 次要数据
    (5, "家居", "收纳盒", 5, "2026-05-20"),  # Q2 次要数据
    (6, "服装", "衬衫", 30, "2026-07-10"),  # Q3（当季）：留作「问错季度」场景的扩展数据，本例候选未用
    (7, "数码", "耳机", 5, "2026-07-20"),  # Q3 次要数据
    (8, "家居", "收纳盒", 5, "2026-08-01"),  # Q3 次要数据
]


def main():
    if os.path.exists(DB):
        os.remove(DB)
    conn = sqlite3.connect(DB)
    conn.execute(
        """
        create table returns (
            id integer primary key,
            category text not null,
            product_name text not null,
            quantity integer not null,
            return_date text not null
        )
        """
    )
    conn.executemany("insert into returns values (?, ?, ?, ?, ?)", ROWS)
    conn.commit()
    conn.close()
    print(f"built {DB}")


if __name__ == "__main__":
    main()
