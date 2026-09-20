"""Validate this plain-text post pack using a conservative weighted upper bound.

URLs count as 23, ASCII as 1, every other code point as 2. This overcounts
some Unicode; it is not a replacement for twitter-text's complete parser.
This pack uses ordinary HTTPS URLs and no emoji sequences.
"""
from pathlib import Path
import json
import re
import unicodedata

ROOT = Path(__file__).resolve().parent

def upper_bound(text):
    text = unicodedata.normalize('NFC', text)
    urls = re.findall(r'https://[^\s]+', text)
    plain = re.sub(r'https://[^\s]+', '', text)
    return 23 * len(urls) + sum(1 if ord(c) < 128 else 2 for c in plain)

def main():
    posts = json.loads((ROOT / 'x-posts.json').read_text())['posts']
    for post in posts:
        length = upper_bound(post['text'])
        assert 0 < length <= 280, (post['id'], length)
    for lang in ('zh-CN', 'en'):
        lines = [f'# J++ X posts — {lang}', '', 'Drafts / 待发布。Copy only the text inside each block / 只复制代码块中的正文。', '']
        for post in (p for p in posts if p['language'] == lang):
            lines += [f"## {post['id']} · {post['title']}", '',
                      f"Weighted upper bound / 加权长度上界：{upper_bound(post['text'])}/280", '',
                      '```text', post['text'], '```', '']
        (ROOT / f'x-posts.{lang}.md').write_text('\n'.join(lines))
    print(f'{len(posts)} drafts checked; maximum weighted upper bound = {max(upper_bound(p["text"]) for p in posts)}/280')

if __name__ == '__main__':
    main()
