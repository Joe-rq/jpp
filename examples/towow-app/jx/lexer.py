"""J++x 词法：.jpx 源码 → 记号流。名字支持 Unicode 字母；`//` 注释；字符串双引号带 \\n \\t \\" \\\\ 转义。"""
from __future__ import annotations

from dataclasses import dataclass


class JxSyntaxError(Exception):
    def __init__(self, msg: str, line: int, col: int, file: str = "<src>", src: str | None = None):
        self.msg, self.line, self.col, self.file, self.src = msg, line, col, file, src
        super().__init__(self.render())

    def render(self) -> str:
        s = f"{self.file}:{self.line}:{self.col}: 语法错误：{self.msg}"
        if self.src:
            lines = self.src.splitlines()
            if 0 < self.line <= len(lines):
                ln = lines[self.line - 1]
                s += f"\n  {ln}\n  {' ' * (self.col - 1)}^"
        return s


KEYWORDS = {
    "let", "fn", "if", "else", "budget", "import", "true", "false", "unit",
    "cell", "resident", "on", "reducer", "peek", "snap", "settled", "put", "claim", "spawn", "ev",
}

# 多字符符号，长的在前
SYMBOLS = ["??", "<-", "->", "==", "!=", "<=", ">=", "&&", "||",
           "{", "}", "(", ")", "[", "]", ",", ";", ":", ".", "=", "<", ">", "+", "-", "*", "/", "%", "!"]


@dataclass
class Tok:
    kind: str    # ident | kw | num | str | sym | eof
    val: object
    line: int
    col: int

    def __repr__(self):
        return f"{self.kind}:{self.val!r}@{self.line}:{self.col}"


def _is_ident_start(c: str) -> bool:
    return c == "_" or c.isalpha()


def _is_ident_char(c: str) -> bool:
    return c == "_" or c.isalnum()


def lex(src: str, file: str = "<src>") -> list[Tok]:
    toks: list[Tok] = []
    i, line, col = 0, 1, 1
    n = len(src)

    def err(msg):
        raise JxSyntaxError(msg, line, col, file, src)

    while i < n:
        c = src[i]
        if c == "\n":
            i += 1; line += 1; col = 1
            continue
        if c in " \t\r﻿　":
            i += 1; col += 1
            continue
        if src.startswith("//", i):
            while i < n and src[i] != "\n":
                i += 1
            continue
        if src.startswith("/*", i):
            j = src.find("*/", i + 2)
            if j < 0:
                err("块注释没有结束")
            seg = src[i:j + 2]
            line += seg.count("\n")
            col = (len(seg) - seg.rfind("\n")) if "\n" in seg else col + len(seg)
            i = j + 2
            continue
        sl, sc = line, col
        if c == '"':
            i += 1; col += 1
            buf = []
            while True:
                if i >= n:
                    raise JxSyntaxError("字符串没有结束", sl, sc, file, src)
                ch = src[i]
                if ch == '"':
                    i += 1; col += 1
                    break
                if ch == "\n":
                    raise JxSyntaxError("字符串里不能直接换行（用 \\n）", sl, sc, file, src)
                if ch == "\\":
                    nx = src[i + 1] if i + 1 < n else ""
                    m = {"n": "\n", "t": "\t", "r": "\r", '"': '"', "\\": "\\"}.get(nx)
                    if m is None:
                        err(f"未知转义 \\{nx}")
                    buf.append(m); i += 2; col += 2
                    continue
                buf.append(ch); i += 1; col += 1
            toks.append(Tok("str", "".join(buf), sl, sc))
            continue
        if c.isdigit():
            j = i
            while j < n and src[j].isdigit():
                j += 1
            isf = False
            if j < n and src[j] == "." and j + 1 < n and src[j + 1].isdigit():
                isf = True
                j += 1
                while j < n and src[j].isdigit():
                    j += 1
            if j < n and src[j] in "eE" and (j + 1 < n and (src[j + 1].isdigit() or src[j + 1] in "+-")):
                isf = True
                j += 1
                if src[j] in "+-":
                    j += 1
                while j < n and src[j].isdigit():
                    j += 1
            txt = src[i:j]
            toks.append(Tok("num", float(txt) if isf else int(txt), sl, sc))
            col += j - i; i = j
            continue
        if _is_ident_start(c):
            j = i + 1
            while j < n and _is_ident_char(src[j]):
                j += 1
            w = src[i:j]
            toks.append(Tok("kw" if w in KEYWORDS else "ident", w, sl, sc))
            col += j - i; i = j
            continue
        for s in SYMBOLS:
            if src.startswith(s, i):
                toks.append(Tok("sym", s, sl, sc))
                i += len(s); col += len(s)
                break
        else:
            err(f"不认识的字符 {c!r}")
    toks.append(Tok("eof", None, line, col))
    return toks
