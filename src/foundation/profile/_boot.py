"""子进程引导：在不改实验脚本与 core 的前提下，把模型版本注入 Params.DEFAULTS 与 common.call。
用法（由 run.py 调用）：python -m foundation.profile._boot <model> <module> [argv...]
"""
from __future__ import annotations
import dataclasses, functools, runpy, sys


def main() -> None:
    model, module, *argv = sys.argv[1:]
    import foundation.core.params as params
    d = params.DEFAULTS
    params.DEFAULTS = dataclasses.replace(d, model_version=model) if dataclasses.is_dataclass(d) else d
    if not dataclasses.is_dataclass(d):
        try:
            d.model_version = model
        except Exception:
            pass
    try:
        import foundation.experiments.common as common
        common.call = functools.partial(common.call, model=model)
    except Exception:
        pass
    sys.argv = [module] + argv
    runpy.run_module(module, run_name="__main__", alter_sys=True)


if __name__ == "__main__":
    main()
