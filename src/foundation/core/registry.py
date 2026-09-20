"""代码函数登记表。

试验台的 `hands.py` 用 `@register("名字")` 登记 prefilter / code 眼 / code 手；
core 不 import testbeds，由 CLI 按路径 importlib 加载试验台模块。
加载器宽容：找不到登记名时回退到模块级同名函数。
"""

from __future__ import annotations

import importlib.util
import os
import sys
from types import ModuleType
from typing import Callable

_REGISTRY: dict[str, Callable] = {}
_MODULES: list[ModuleType] = []


def register(name: str) -> Callable[[Callable], Callable]:
    def deco(fn: Callable) -> Callable:
        _REGISTRY[name] = fn
        return fn
    return deco


def register_fn(name: str, fn: Callable) -> None:
    _REGISTRY[name] = fn


def load_module(path: str, mod_name: str | None = None) -> ModuleType | None:
    """按文件路径加载一个 hands.py，并记下来供 getattr 回退。"""
    if not os.path.exists(path):
        return None
    mod_name = mod_name or ("testbed_hands_" + str(abs(hash(os.path.abspath(path)))))
    spec = importlib.util.spec_from_file_location(mod_name, path)
    if spec is None or spec.loader is None:
        return None
    mod = importlib.util.module_from_spec(spec)
    sys.modules[mod_name] = mod
    spec.loader.exec_module(mod)
    _MODULES.append(mod)
    return mod


def lookup(name: str) -> Callable:
    if name in _REGISTRY:
        return _REGISTRY[name]
    for mod in reversed(_MODULES):
        fn = getattr(mod, name, None)
        if callable(fn):
            return fn
    raise KeyError(f"代码函数未登记也未在试验台模块中找到：{name}")


def has(name: str) -> bool:
    try:
        lookup(name)
        return True
    except KeyError:
        return False


def reset() -> None:
    _REGISTRY.clear()
    _MODULES.clear()
