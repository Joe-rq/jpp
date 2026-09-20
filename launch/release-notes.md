# J++ 0.1.0a1 — first experimental release

J++ explores questions and solving methods as composable values. This alpha packages the Python embedded implementation, judgment runtime and composition library into an installable distribution.

## Try it

Clone the repository and follow the README, or install the attached wheel with Python 3.12+:

```sh
python -m pip install ./jpp_language-0.1.0a1-py3-none-any.whl
jpp demo
```

The offline demo identifies target 731 among 1,000 candidates in 10 questions, then constructs `x if x >= 0 else -x` in three trials and checks all nine declared inputs. Observations are synthetic, candidate generation uses finite enumeration, and API cost is zero.

The source includes 25 passing mechanism tests, question serialization, nested component composition, explicit uncertainty, and an independently authored allocation method. This release does not include independent syntax or a standalone compiler. Real-model calibration and backend portability remain active design work.

中文：这是 J++ 的首个实验版本，提供可运行的 Python 嵌入式语言、运行内核和组合库。欢迎用已有组件构造新方法，帮助我们发现语言下一步应该长成什么样。
