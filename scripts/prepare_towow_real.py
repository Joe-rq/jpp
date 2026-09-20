"""Prepare identifier-minimized public-source research data; no network access."""
import argparse
from hashlib import sha256
import json
from pathlib import Path
import re


def prepare(root, destination):
    paths=sorted((root/'profiles/normalized').glob('*.json'))
    profiles=[json.loads(p.read_text()) for p in paths]
    ids={p['profile_id']:f'p{i+1:03d}' for i,p in enumerate(profiles)}
    personal=sorted({str(p.get(k,'')) for p in profiles for k in ('name','username') if len(str(p.get(k,'')))>2},key=len,reverse=True)
    email=re.compile(r'[\w.+-]+@[\w.-]+\.[A-Za-z]{2,}')
    url=re.compile(r'https?://\S+')
    people=[]
    for p in profiles:
        text=url.sub('[link removed]',email.sub('[contact removed]',p['normalized_text']))
        for value in personal:
            text=re.sub(r'(?<!\w)'+re.escape(value)+r'(?!\w)','[person]',text,flags=re.I)
        people.append({'id':ids[p['profile_id']],'source_type':p['source_type'],'context':text})
    label_path=root/'relations/ground_truth.json'
    original=json.loads(label_path.read_text())['relations']
    missing=[r for r in original if r['agent_a'] not in ids or r['agent_b'] not in ids]
    relations=[{'a':ids[r['agent_a']],'b':ids[r['agent_b']],'type':r['relation_type'],
                'label_scope':'curated proxy' if r['relation_type'].startswith('cross_source') else 'source-backed proxy'} for r in original if r not in missing]
    data={'schema':1,'people':people,'known_relations':relations,
          'provenance':{'original_profiles':len(profiles),'original_relations':len(original),
                        'missing_endpoint_relations':len(missing),'label_file_sha256':sha256(label_path.read_bytes()).hexdigest(),
                        'profile_corpus_sha256':sha256(b''.join(p.read_bytes() for p in paths)).hexdigest()},
          'scope':'Public-source professional descriptions, with direct personal identifiers removed. Shared paper/company signals remain; not anonymous and not an unseen-cooperation prediction test. Labels are evaluation-only.'}
    assert not any(email.search(p['context']) or url.search(p['context']) for p in people)
    destination.parent.mkdir(parents=True,exist_ok=True)
    destination.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
    print(f'{len(people)} profiles, {len(relations)} usable relations, {len(missing)} missing-endpoint relations; no email/URL patterns')


if __name__ == '__main__':
    p=argparse.ArgumentParser()
    p.add_argument('source',type=Path)
    p.add_argument('destination',type=Path)
    a=p.parse_args()
    prepare(a.source,a.destination)
