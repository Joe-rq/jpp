"""测试夹具。假编码器：汉字单字/二元组 + 英文词哈希到 512 维再归一化——确定、快，字面相近得到高余弦。
只测机制（增删、排除、汇总、路由方向）；语义价值由 test_index_bge.py（slow，真 bge-m3）测。"""
import hashlib
import os
import sys

import numpy as np
import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)

from host.graph import _grams  # noqa: E402


class FakeEnc:
    dim = 512

    def __init__(self):
        self.calls = 0

    def encode(self, texts):
        self.calls += 1
        out = np.zeros((len(texts), self.dim), np.float32)
        for i, t in enumerate(texts):
            for g in _grams(t):
                hv = int(hashlib.md5(g.encode()).hexdigest()[:8], 16)
                out[i, hv % self.dim] += 1.0 if len(g) > 1 else 0.3
            n = np.linalg.norm(out[i])
            if n == 0:
                out[i, 0] = 1.0
                n = 1.0
            out[i] /= n
        return out


@pytest.fixture
def fake_enc():
    return FakeEnc()


def pytest_configure(config):
    config.addinivalue_line("markers", "slow: 需要真 bge-m3 或长时间运行")
