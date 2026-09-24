"""Document contract/size check; no Nanoom runtime, network, or S3 validation.

uv run --no-project --with openapi-spec-validator --with pyyaml --with rfc8785 \
  python docs/validation/check_prediction_spec.py
"""
from copy import deepcopy
from io import BytesIO
from pathlib import Path
import hashlib
import json
import math
import re
import zipfile

import rfc8785
import yaml
from jsonschema import Draft202012Validator, FormatChecker, RefResolver, ValidationError
from openapi_spec_validator import validate

ROOT = Path(__file__).resolve().parents[2]
API = yaml.safe_load((ROOT / 'docs/api/history.openapi.yaml').read_text())
BASE = '/v1/repositories/{repositoryKey}/scopes/{scopeId}'
SCHEMAS = API['components']['schemas']
RESOLVER = RefResolver.from_schema(API)
SHA = lambda value: hashlib.sha256(rfc8785.dumps(value)).hexdigest()


def validator(schema):
    return Draft202012Validator(schema, resolver=RESOLVER, format_checker=FormatChecker())


def check(name, value):
    validator({'$ref': '#/components/schemas/' + name}).validate(value)


def document_check():
    validate(API)
    for schema in SCHEMAS.values():
        Draft202012Validator.check_schema(schema)
    count = 0

    def walk(node):
        nonlocal count
        if isinstance(node, dict):
            if 'schema' in node:
                v = validator(node['schema'])
                values = ([node['example']] if 'example' in node else [])
                values += [ex['value'] for ex in node.get('examples', {}).values() if 'value' in ex]
                for value in values:
                    v.validate(value)
                    count += 1
            for value in node.values():
                walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(API)
    for name, schema in SCHEMAS.items():
        for example in schema.get('examples', []):
            check(name, example)
            count += 1
    batch = API['paths'][BASE + '/observations:merge']['post']['requestBody']['content']['application/json']['examples']['successfulAttempt']['value']
    table = API['paths'][BASE + '/snapshot']['get']['responses']['200']['content']['application/json']['example']
    model = SCHEMAS['ModelState']['examples'][0]
    vectors = API['x-contract-examples']
    assert SHA(batch['scope']) == vectors['scopeId']
    assert SHA(vectors['key']) == vectors['keyId']
    assert SHA([vectors['scopeId'], batch['runId'], batch['runAttempt']]) == vectors['batchId']
    assert SHA(batch) == vectors['idempotencyKeyForCanonicalBatch']
    assert '"sha256:' + SHA(table) + '"' == vectors['predictionEtag']
    assert SHA(model) == vectors['modelDigest']
    invalid = []
    for field, value in [('unexpected', True), ('aggregates', []), ('version', 2), ('batchId', 'bad'), ('runAttempt', 0)]:
        item = deepcopy(batch)
        item[field] = value
        invalid.append(('ObservationBatch', item))
    for field in ('observationCount', 'totalDurationMs'):
        item = deepcopy(batch)
        item['aggregates'][0][field] = -1
        invalid.append(('ObservationBatch', item))
    invalid += [('PredictionRow', table['rows'][0][:-1]), ('PredictionTable', {**table, 'samples': []})]
    for name, value in invalid:
        try:
            check(name, value)
        except ValidationError:
            continue
        raise AssertionError(f'Invalid {name} was accepted')
    links = 0
    for file in ('LUNA_HANDOFF.md', 'IMPLEMENTATION_PLAN.md', 'SPEC.md', 'CHECKLIST.md', 'docs/prediction-model-spec.md', 'docs/history-server-spec.md'):
        path = ROOT / file
        for link in re.findall(r'\]\(([^)]+)\)', path.read_text()):
            if '://' in link or link.startswith('#'):
                continue
            assert (path.parent / link.split('#')[0]).is_file(), (file, link)
            links += 1
    assert set(API['paths']) == {'/health', '/ready', BASE + '/snapshot', BASE + '/observations:merge'}
    assert API['x-runtime-limits']['plannerHistoryBudgetMs'] == 3000
    print(f'PASS OpenAPI: {len(SCHEMAS)} schemas, {count} examples, 6 JCS vectors, {len(invalid)} invalid cases, {links} local links')


def predict(buckets):
    """Design prototype only; Rust implementation must get its own acceptance evidence."""
    latest = max(b[0] for b in buckets)
    weights = [2 ** (-(latest - b[0]) / 7) for b in buckets]
    mean = sum(b[2] * w for b, w in zip(buckets, weights)) / sum(b[1] * w for b, w in zip(buckets, weights))
    return [math.floor(mean + .5), sum(b[1] for b in buckets), max(b[3] for b in buckets), (min(b[0] for b in buckets) + 30) * 86400000]


def sizes(value):
    data = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode()
    buf = BytesIO()
    with zipfile.ZipFile(buf, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        archive.writestr('payload.json', data)
    return len(data), len(buf.getvalue())


def size_check():
    scope = SCHEMAS['ModelState']['examples'][0]['scope']
    now = API['x-contract-examples']['clockMs']
    day = now // 86400000
    entries = []
    for index in range(30103):
        key = {**API['x-contract-examples']['key'], 'workspace': f'@fixture/workspace-{index:05d}'}
        if index >= 30000:  # Three task fallbacks + 100 preparation exact/fallback keys.
            key = {'kind': 'taskFallback', 'group': 'ci', 'task': ('build', 'test', 'lint')[(index - 30000) % 3], 'shard': None, 'totalShards': None, 'taskRunner': 'yarn', 'timingEnvironment': 'linux-x64-node24'} if index < 30003 else {
                'kind': 'preparationExact' if index % 2 else 'preparationFallback', 'group': f'group-{index}', 'taskRunner': 'yarn', 'timingEnvironment': 'linux-x64-node24', 'packageManager': 'yarn', 'packageManagerVersion': '4.9.2', 'installMode': 'focused', 'lockfileDigest': SHA(index)}
            if key['kind'] == 'preparationExact':
                key.update(checkoutDigest=SHA([index]), workspaceSetDigest=SHA([str(index)]))
        buckets = [[day - offset, 1, 1000 + ((index * 7919 + offset * 104729) % 300000), now - offset * 86400000] for offset in range(6, -1, -1)]
        entries.append({'keyId': SHA(key), 'buckets': buckets, 'prediction': predict(buckets)})
    receipts = sorted([[SHA(['batch', i]), SHA(['body', i]), now - (i % 8) * 86400000] for i in range(4096)])
    print('taskKeys,totalKeys,raw7RecordsJSON,raw7RecordsZIP,modelJSON,modelZIP,predictionJSON,predictionZIP')
    for count in (1000, 10000, 30000):
        selected = sorted(entries[:count] + entries[30000:], key=lambda e: e['keyId'])
        model = {'version': 3, 'scope': scope, 'updatedAtMs': now, 'pruningDay': day - 29, 'batchAcceptanceAfterMs': now - 7 * 86400000, 'entries': selected, 'receipts': receipts}
        table = {'version': 3, 'scope': scope, 'modelUpdatedAtMs': now, 'rows': [[e['keyId'], *e['prediction']] for e in selected]}
        artifact = {'table': table, 'modelArtifact': {'name': 'nanoom-model-v3-123456789-1', 'sha256': SHA(model)}}
        # Synthetic raw-record shape for size comparison, not a v0.6 serialization benchmark.
        raw = {'scope': scope, 'samples': [dict(keyId=e['keyId'], durationMs=b[2], observedAtMs=b[3], executionId=SHA([e['keyId'], b[0]]), source={'repositoryKey': scope['repositoryKey'], 'workflowPath': scope['workflowPath'], 'ref': scope['ref'], 'runId': str(123456789 + b[0]), 'runAttempt': 1, 'assignmentId': 'ci-0001', 'headSha': hashlib.sha256(str(b[0]).encode()).hexdigest()[:40]}) for e in selected for b in e['buckets']]}
        check('ModelState', model)
        check('PredictionArtifact', artifact)
        raw_size, model_size, prediction_size = sizes(raw), sizes(model), sizes(artifact)
        assert model_size[0] <= API['x-runtime-limits']['modelBytes']
        assert prediction_size[0] <= API['x-runtime-limits']['predictionJsonBytes']
        assert prediction_size[1] <= API['x-runtime-limits']['predictionArchiveBytes']
        print(','.join(map(str, (count, len(selected), *raw_size, *model_size, *prediction_size))))
    repeated = deepcopy(entries[0])
    for b in repeated['buckets']:
        b[1] *= 100000
        b[2] *= 100000
    repeated['prediction'] = predict(repeated['buckets'])
    assert len(repeated['buckets']) == 7
    assert repeated['prediction'][0] == entries[0]['prediction'][0]
    print(f'PASS repeated observations: 7 -> 700000, buckets remain 7, entry bytes {sizes(entries[0])[0]} -> {sizes(repeated)[0]} (integer digit growth only)')
    print('NOT VALIDATED: Rust semantics, prediction error, network latency, real CI, S3 CAS, producer/consumer/release paths.')


if __name__ == '__main__':
    document_check()
    size_check()
