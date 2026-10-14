// Generates the CLI golden corpus (crates/mdg-cli/tests/golden/cases.json)
// by running the original TypeScript CLI in-process. The Rust CLI tests
// replay every case and require identical exit codes, stdout, stderr and
// written files.
//
// Usage: node scripts/gen-golden.mjs <path-to-typescript-MDG-repo>
// (the TypeScript repo must be built: `npm run build`).
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const tsRepo = path.resolve(process.argv[2] ?? '../MDG');
const here = path.dirname(fileURLToPath(import.meta.url));
const out = path.join(here, '..', 'crates', 'mdg-cli', 'tests', 'golden', 'cases.json');
const load = (p) => import(pathToFileURL(path.join(tsRepo, p)).href);
const core = await load('packages/core/dist/index.js');
const node = await load('packages/core/dist/node/index.js');
const { runCli } = await load('packages/cli/dist/cli.js');

const UUID = /\b(mod|port|conn)-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/g;
/** Generated IDs and the per-case working directory differ between runs. */
const normalize = (s, dir) => s.split(dir).join('<dir>').replace(UUID, '$1-<uuid>');

/** Fixtures available to cases. */
const setups = {
  none: () => {},
  example: () => node.writeMDG('arch.mdg', core.createAuthenticationExample()),
  bad: () => {
    node.writeMDG('arch.mdg', core.createAuthenticationExample());
    const doc = core.createAuthenticationExample();
    doc.modules.application.childIds = ['frontend', 'api-service'];
    fs.writeFileSync('bad.mdg', core.encodeMDG(doc, { validate: false }));
  },
  garbage: () => fs.writeFileSync('garbage.mdg', 'not an mdg file at all, definitely not'),
  cycle: () => {
    let doc = core.createDocument({ title: 'Cycle', rootId: 'root' });
    const add = (id) => (doc = core.addModule(doc, { id, parentId: 'root', name: id.toUpperCase() }).document);
    const port = (m, d, id) => (doc = core.addPort(doc, m, d, { id, name: `${m} ${id}` }).document);
    const conn = (id, f, t) => {
      const [fm, fp] = f.split(':');
      const [tm, tp] = t.split(':');
      doc = core.addConnection(doc, { id, from: { moduleId: fm, portId: fp }, to: { moduleId: tm, portId: tp } }).document;
    };
    add('a');
    add('b');
    port('a', 'input', 'in');
    port('a', 'output', 'out');
    port('b', 'input', 'in');
    port('b', 'output', 'out');
    conn('a-pass', 'a:in', 'a:out');
    conn('b-pass', 'b:in', 'b:out');
    conn('ab', 'a:out', 'b:in');
    conn('ba', 'b:out', 'a:in');
    node.writeMDG('cycle.mdg', doc);
  },
  numeric: () => {
    let doc = core.createDocument({ title: 'Numbers', rootId: 'root' });
    for (const id of ['b', '10', 'a', '9', '10a']) doc = core.addModule(doc, { id, parentId: 'root', name: `N${id}` }).document;
    node.writeMDG('num.mdg', doc);
  },
  description: () => {
    node.writeMDG('arch.mdg', core.createAuthenticationExample());
    fs.writeFileSync('desc.md', '# From file\n\nLoaded from **disk**.\n');
  },
};

const cases = [];
/**
 * name, setup, steps: [{ args, stdin? }], files: names to capture afterwards,
 * bytes: compare written .mdg bytes (false when random IDs are generated).
 */
const add = (name, setup, steps, files = [], bytes = true) =>
  cases.push({ name, setup, steps: steps.map((s) => (Array.isArray(s) ? { args: s } : s)), files, bytes });
const one = (name, setup, args, files = [], bytes = true) => add(name, setup, [args], files, bytes);

// ---------------------------------------------------------------- help and usage
const leaf = [
  ['info'], ['tree'], ['describe'], ['inputs'], ['outputs'], ['connections'], ['trace'], ['validate'], ['export'],
  ['new'], ['module', 'add'], ['module', 'remove'], ['module', 'rename'], ['module', 'move'],
  ['module', 'set-description'], ['port', 'add'], ['port', 'remove'], ['port', 'rename'],
  ['port', 'set-description'], ['connect'], ['disconnect'],
];
one('help', 'none', ['--help']);
one('help short', 'none', ['-h']);
one('help command', 'none', ['help']);
one('no arguments', 'none', []);
one('version', 'none', ['--version']);
one('version short', 'none', ['-V']);
one('version anywhere', 'example', ['info', 'arch.mdg', '-V']);
one('help for module via help', 'none', ['help', 'module']);
one('help for connect via help', 'none', ['help', 'connect']);
one('help for unknown via help', 'none', ['help', 'nope']);
one('help module add via help', 'none', ['help', 'module', 'add']);
one('module without subcommand', 'none', ['module']);
one('module help flag', 'none', ['module', '--help']);
one('module help command', 'none', ['module', 'help']);
one('module help add', 'none', ['module', 'help', 'add']);
one('port without subcommand', 'none', ['port']);
one('port help flag', 'none', ['port', '-h']);
for (const cmd of leaf) one(`help ${cmd.join(' ')}`, 'none', [...cmd, '--help']);
one('help before command', 'none', ['--help', 'info']);
one('help wins over errors', 'none', ['describe', 'x', '--depth', '1', '--help']);

// ---------------------------------------------------------------- usage errors
one('unknown command', 'none', ['nope']);
one('unknown command suggestion', 'none', ['inf']);
one('unknown command suggestion 2', 'none', ['conect']);
one('unknown command with json', 'none', ['nope', '--json']);
one('unknown program option', 'none', ['--nope']);
one('unknown program option then command', 'none', ['--nope', 'info']);
one('unknown option suggestion', 'example', ['info', 'arch.mdg', '--jsn']);
one('unknown short option', 'example', ['info', 'arch.mdg', '-x']);
one('unknown option before args', 'example', ['tree', '--idz', 'arch.mdg']);
one('missing argument', 'none', ['info']);
one('missing second argument', 'example', ['describe', 'arch.mdg']);
one('too many arguments', 'example', ['info', 'arch.mdg', 'extra']);
one('too many arguments 2', 'example', ['trace', 'arch.mdg', 'a', 'b', 'c']);
one('invalid integer', 'example', ['tree', 'arch.mdg', '--depth', 'x']);
one('negative integer', 'example', ['tree', 'arch.mdg', '--depth', '-1']);
one('option missing value', 'example', ['tree', 'arch.mdg', '--depth']);
one('option equals value', 'example', ['tree', 'arch.mdg', '--depth=1']);
one('positive integer required', 'example', ['trace', 'arch.mdg', 'jwt-validator', 'validate-token', '--max-depth', '0']);
one('invalid choice', 'example', ['export', 'arch.mdg', '--format', 'html']);
one('missing mandatory option', 'example', ['port', 'add', 'arch.mdg', 'frontend', '--name', 'x']);
one('mandatory before arguments', 'none', ['module', 'add']);
one('invalid direction choice', 'example', ['port', 'add', 'arch.mdg', 'frontend', '--direction', 'sideways', '--name', 'x']);
one('new without title', 'none', ['new', 'x.mdg']);
one('module unknown subcommand', 'none', ['module', 'nope']);
one('module unknown subcommand suggestion', 'none', ['module', 'ad']);
one('via requires route', 'example', ['connect', 'arch.mdg', 'frontend:login-submitted', 'api-service:login-request', '--via', 'x:y']);
one('no-reuse requires route', 'example', ['connect', 'arch.mdg', 'a:b', 'c:d', '--no-reuse', '--json']);
one('route excludes id', 'example', ['connect', 'arch.mdg', 'a:b', 'c:d', '--route', '--id', 'x']);
one('description and file', 'example', ['module', 'set-description', 'arch.mdg', 'frontend', '--description', 'x', '--description-file', 'y']);
one('set-description needs text', 'example', ['module', 'set-description', 'arch.mdg', 'frontend']);
one('set-description needs text json', 'example', ['port', 'set-description', 'arch.mdg', 'frontend', 'login-submitted', '--json']);
one('double dash', 'example', ['info', '--', 'arch.mdg']);

// ---------------------------------------------------------------- inspection
one('info', 'example', ['info', 'arch.mdg']);
one('info json', 'example', ['info', 'arch.mdg', '--json']);
one('info missing file', 'none', ['info', 'missing.mdg']);
one('info missing file json', 'none', ['info', 'missing.mdg', '--json']);
one('info garbage', 'garbage', ['info', 'garbage.mdg']);
one('tree', 'example', ['tree', 'arch.mdg']);
one('tree ids', 'example', ['tree', 'arch.mdg', '--ids']);
one('tree depth 0', 'example', ['tree', 'arch.mdg', '--depth', '0']);
one('tree depth 1 ids', 'example', ['tree', 'arch.mdg', '--ids', '--depth', '1']);
one('tree json', 'example', ['tree', 'arch.mdg', '--json']);
one('tree depth json', 'example', ['tree', '--depth', '1', '--json', 'arch.mdg']);
one('tree numeric ids', 'numeric', ['tree', 'num.mdg', '--ids']);
one('describe', 'example', ['describe', 'arch.mdg', 'authentication']);
one('describe root', 'example', ['describe', 'arch.mdg', 'application']);
one('describe depth 1', 'example', ['describe', 'arch.mdg', 'authentication', '--depth', '1']);
one('describe depth 2 json', 'example', ['describe', 'arch.mdg', 'api-service', '--depth', '2', '--json']);
one('describe all depth', 'example', ['describe', 'arch.mdg', 'application', '--depth', '9']);
one('describe prefix', 'example', ['describe', 'arch.mdg', 'jwt']);
one('describe ambiguous', 'example', ['describe', 'arch.mdg', 'a']);
one('describe ambiguous json', 'example', ['describe', 'arch.mdg', 'a', '--json']);
one('describe missing', 'example', ['describe', 'arch.mdg', 'zzz']);
one('describe numeric ambiguous', 'numeric', ['describe', 'num.mdg', '1']);
one('describe output', 'example', ['describe', 'arch.mdg', 'api-service', '--output', 'api.md'], ['api.md']);
one('describe output json', 'example', ['describe', 'arch.mdg', 'api-service', '--json', '--output', 'api.json'], ['api.json']);
one('inputs', 'example', ['inputs', 'arch.mdg', 'api-service']);
one('inputs json', 'example', ['inputs', 'arch.mdg', 'jwt-validator', '--json']);
one('outputs', 'example', ['outputs', 'arch.mdg', 'jwt-validator']);
one('outputs root', 'example', ['outputs', 'arch.mdg', 'application']);
one('connections', 'example', ['connections', 'arch.mdg', 'user-routes']);
one('connections json', 'example', ['connections', 'arch.mdg', 'user-routes', '--json']);
one('connections none', 'example', ['connections', 'arch.mdg', 'frontend']);
one('connections root', 'example', ['connections', 'arch.mdg', 'application']);
one('trace', 'example', ['trace', 'arch.mdg', 'protected-endpoint', 'token-validation-request']);
one('trace json', 'example', ['trace', 'arch.mdg', 'protected-endpoint', 'token-validation-request', '--json']);
one('trace reverse json', 'example', ['trace', 'arch.mdg', 'api-service', 'login-response', '--reverse', '--json']);
one('trace reverse', 'example', ['trace', 'arch.mdg', 'api-service', 'login-response', '--reverse']);
one('trace none', 'example', ['trace', 'arch.mdg', 'frontend', 'session-established']);
one('trace prefix port', 'example', ['trace', 'arch.mdg', 'jwt', 'valid']);
one('trace max routes', 'example', ['trace', 'arch.mdg', 'api-service', 'login-response', '--reverse', '--max-routes', '1']);
one('trace max depth', 'example', ['trace', 'arch.mdg', 'protected-endpoint', 'token-validation-request', '--max-depth', '1']);
one('trace cycle', 'cycle', ['trace', 'cycle.mdg', 'a', 'in']);
one('trace cycle json', 'cycle', ['trace', 'cycle.mdg', 'a', 'in', '--json']);
one('trace bad port json', 'example', ['trace', 'arch.mdg', 'jwt-validator', 'nope', '--json']);
one('trace ambiguous port', 'example', ['trace', 'arch.mdg', 'api-service', 'login']);
one('validate', 'example', ['validate', 'arch.mdg']);
one('validate json', 'example', ['validate', 'arch.mdg', '--json']);
one('validate invalid', 'bad', ['validate', 'bad.mdg']);
one('validate invalid json', 'bad', ['validate', 'bad.mdg', '--json']);
one('export', 'example', ['export', 'arch.mdg']);
one('export format', 'example', ['export', 'arch.mdg', '--format', 'markdown']);
one('export output', 'example', ['export', 'arch.mdg', '--output', 'ARCH.md'], ['ARCH.md']);
one(
  'export split',
  'example',
  ['export', 'arch.mdg', '--split', 'docs'],
  ['docs/index.md', 'docs/application.md', 'docs/api-service.md', 'docs/user-routes.md', 'docs/protected-endpoint.md'],
);
one('edit invalid document', 'bad', ['module', 'rename', 'bad.mdg', 'frontend', 'X']);
one('edit invalid document json', 'bad', ['module', 'rename', 'bad.mdg', 'frontend', 'X', '--json']);

// ---------------------------------------------------------------- editing
add('new and build', 'none', [
  ['new', 'shop.mdg', '--title', 'Shop', '--root-id', 'shop'],
  ['new', 'shop.mdg', '--title', 'Again'],
  ['module', 'add', 'shop.mdg', '--parent', 'shop', '--name', 'Cart', '--id', 'cart', '--json', '--description', 'Holds items.'],
  ['module', 'add', 'shop.mdg', '--parent', 'shop', '--name', 'Checkout', '--id', 'checkout'],
  ['module', 'add', 'shop.mdg', '--parent', 'shop', '--name', 'First', '--id', 'first', '--index', '0'],
  ['port', 'add', 'shop.mdg', 'cart', '--direction', 'output', '--name', 'Cart Ready', '--id', 'ready'],
  ['port', 'add', 'shop.mdg', 'checkout', '--direction', 'input', '--name', 'Start', '--id', 'start', '--json'],
  ['connect', 'shop.mdg', 'cart:ready', 'checkout:start', '--id', 'c1', '--json'],
  ['connect', 'shop.mdg', 'cart:ready', 'checkout:start', '--id', 'c2'],
  ['module', 'rename', 'shop.mdg', 'checkout', '  Checkout Flow  '],
  ['port', 'rename', 'shop.mdg', 'checkout', 'start', 'Begin', '--json'],
  { args: ['module', 'set-description', 'shop.mdg', 'checkout', '--description-file', '-'], stdin: '# Checkout\nTakes payment.' },
  ['port', 'set-description', 'shop.mdg', 'checkout', 'start', '--description', 'Receives the cart.'],
  ['validate', 'shop.mdg'],
  ['disconnect', 'shop.mdg', 'c1', '--dry-run'],
  ['disconnect', 'shop.mdg', 'c1'],
  ['disconnect', 'shop.mdg', 'c1'],
  ['tree', 'shop.mdg', '--ids'],
], ['shop.mdg']);
add('new variants', 'description', [
  ['new', 'a.mdg', '--title', '  Spaced  ', '--root-id', 'r', '--root-name', 'Root Module', '--json'],
  ['new', 'a.mdg', '--title', 'Forced', '--root-id', 'r2', '--force', '--description-file', 'desc.md'],
  ['new', 'b.mdg', '--title', 'Bad', '--root-id', 'bad:id'],
  ['new', 'c.mdg', '--title', 'Missing', '--description-file', 'nope.md'],
  ['describe', 'a.mdg', 'r2'],
], ['a.mdg']);
one('new generated root', 'none', ['new', 'gen.mdg', '--title', 'Generated'], [], false);
add('module add generated id', 'example', [
  ['module', 'add', 'arch.mdg', '--parent', 'api', '--name', 'Rate Limiter', '--json'],
  ['module', 'add', 'arch.mdg', '--parent', 'application', '--name', 'Second'],
], [], false);
add('module add errors', 'example', [
  ['module', 'add', 'arch.mdg', '--parent', 'application', '--name', 'x', '--id', 'a:b'],
  ['module', 'add', 'arch.mdg', '--parent', 'application', '--name', '   ', '--id', 'blank'],
  ['module', 'add', 'arch.mdg', '--parent', 'nowhere', '--name', 'x'],
  ['module', 'add', 'arch.mdg', '--parent', 'application', '--name', 'Dup', '--id', 'frontend', '--json'],
  ['module', 'add', 'arch.mdg', '--parent', 'application', '--name', 'x', '--index', 'one'],
], ['arch.mdg']);
add('module remove', 'example', [
  ['module', 'remove', 'arch.mdg', 'authentication'],
  ['module', 'remove', 'arch.mdg', 'authentication', '--json'],
  ['module', 'remove', 'arch.mdg', 'authentication', '--cascade', '--dry-run', '--json'],
  ['module', 'remove', 'arch.mdg', 'authentication', '--cascade', '--dry-run'],
  ['module', 'remove', 'arch.mdg', 'application'],
  ['module', 'remove', 'arch.mdg', 'token', '--cascade'],
  ['module', 'remove', 'arch.mdg', 'authentication', '--cascade'],
  ['tree', 'arch.mdg'],
], ['arch.mdg']);
add('module move', 'example', [
  ['module', 'move', 'arch.mdg', 'protected-endpoint', '--to', 'application'],
  ['module', 'move', 'arch.mdg', 'api-service', '--to', 'user-routes'],
  ['module', 'move', 'arch.mdg', 'application', '--to', 'frontend'],
  ['module', 'add', 'arch.mdg', '--parent', 'application', '--name', 'Audit Log', '--id', 'audit'],
  ['module', 'move', 'arch.mdg', 'audit', '--to', 'api-service', '--json', '--dry-run'],
  ['module', 'move', 'arch.mdg', 'audit', '--to', 'api-service', '--index', '0'],
  ['module', 'move', 'arch.mdg', 'jwt-validator', '--to', 'application', '--index', '0'],
  ['tree', 'arch.mdg', '--ids'],
], ['arch.mdg']);
add('module rename and describe', 'description', [
  ['module', 'rename', 'arch.mdg', 'frontend', 'Web App'],
  ['module', 'rename', 'arch.mdg', 'frontend', ' ', '--json'],
  ['module', 'rename', 'arch.mdg', 'nope', 'X'],
  ['module', 'set-description', 'arch.mdg', 'frontend', '--description-file', 'desc.md', '--json'],
  ['module', 'set-description', 'arch.mdg', 'jwt-validator', '--description', '', '--dry-run'],
  ['module', 'set-description', 'arch.mdg', 'jwt-validator', '--description', ''],
  ['describe', 'arch.mdg', 'frontend'],
  ['describe', 'arch.mdg', 'jwt-validator'],
  ['module', 'set-description', 'arch.mdg', 'frontend', '--description-file', 'missing.md'],
], ['arch.mdg']);
add('port edits', 'example', [
  ['port', 'add', 'arch.mdg', 'frontend', '--direction', 'input', '--name', 'Spare', '--id', 'spare'],
  ['validate', 'arch.mdg'],
  ['validate', 'arch.mdg', '--strict'],
  ['validate', 'arch.mdg', '--no-warnings'],
  ['validate', 'arch.mdg', '--json'],
  ['port', 'add', 'arch.mdg', 'application', '--direction', 'input', '--name', 'x'],
  ['port', 'add', 'arch.mdg', 'frontend', '--direction', 'output', '--name', 'Dup', '--id', 'spare'],
  ['port', 'remove', 'arch.mdg', 'frontend', 'login-submitted'],
  ['port', 'remove', 'arch.mdg', 'frontend', 'login-submitted', '--cascade', '--json'],
  ['port', 'remove', 'arch.mdg', 'frontend', 'spare'],
  ['port', 'remove', 'arch.mdg', 'frontend', 'nope'],
  ['port', 'rename', 'arch.mdg', 'frontend', 'profile-req', 'Profile Wanted'],
  ['port', 'set-description', 'arch.mdg', 'frontend', 'profile-requested', '--description', 'New contract.', '--json'],
  ['outputs', 'arch.mdg', 'frontend'],
], ['arch.mdg']);
one('port add generated', 'example', ['port', 'add', 'arch.mdg', 'frontend', '--direction', 'output', '--name', 'Gen', '--json'], [], false);
add('connect and disconnect', 'example', [
  ['port', 'add', 'arch.mdg', 'frontend', '--direction', 'output', '--name', 'Ping', '--id', 'ping'],
  ['port', 'add', 'arch.mdg', 'jwt-validator', '--direction', 'input', '--name', 'Pong', '--id', 'pong'],
  ['connect', 'arch.mdg', 'frontend:ping', 'jwt-validator:pong', '--id', 'p1', '--dry-run'],
  ['connect', 'arch.mdg', 'frontend:ping', 'jwt-validator:pong', '--id', 'p1', '--container', 'application', '--json'],
  ['connect', 'arch.mdg', 'frontend:ping', 'jwt-validator:pong', '--id', 'p2'],
  ['connect', 'arch.mdg', 'frontend:ping', 'jwt:validate', '--id', 'root-login'],
  ['connect', 'arch.mdg', 'protected-endpoint:http-response', 'frontend:profile-received'],
  ['connect', 'arch.mdg', 'protected-endpoint:http-response', 'frontend:profile-received', '--json'],
  ['connect', 'arch.mdg', 'frontend:session-established', 'jwt-validator:pong'],
  ['connect', 'arch.mdg', 'frontend:ping', 'jwt-validator:pong', '--container', 'api-service'],
  ['connect', 'arch.mdg', 'frontend', 'jwt-validator:pong'],
  ['connect', 'arch.mdg', 'frontend:', 'jwt-validator:pong'],
  ['connect', 'arch.mdg', ':ping', 'jwt-validator:pong'],
  ['connect', 'arch.mdg', 'frontend:nope', 'jwt-validator:pong'],
  ['connect', 'arch.mdg', 'frontend:ping', 'jwt-validator:pong', '--id', 'bad id'],
  ['disconnect', 'arch.mdg', 'p1', '--json'],
  ['disconnect', 'arch.mdg', 'nope'],
  ['connections', 'arch.mdg', 'application'],
], ['arch.mdg']);
add('route', 'example', [
  ['port', 'add', 'arch.mdg', 'protected-endpoint', '--direction', 'output', '--name', 'Audit Event', '--id', 'audit-event'],
  ['module', 'add', 'arch.mdg', '--parent', 'application', '--name', 'Audit Log', '--id', 'audit'],
  ['port', 'add', 'arch.mdg', 'audit', '--direction', 'input', '--name', 'Record', '--id', 'record'],
  ['connect', 'arch.mdg', 'protected-endpoint:audit-event', 'audit:record', '--route', '--dry-run'],
  ['connect', 'arch.mdg', 'protected-endpoint:audit-event', 'audit:record', '--route', '--dry-run', '--json'],
  ['connect', 'arch.mdg', 'protected-endpoint:audit-event', 'audit:record', '--route', '--output-name', 'Audit Out'],
  ['connect', 'arch.mdg', 'protected-endpoint:audit-event', 'audit:record', '--route'],
  ['connect', 'arch.mdg', 'protected-endpoint:audit-event', 'audit:record', '--route', '--no-reuse', '--json'],
  ['trace', 'arch.mdg', 'protected-endpoint', 'audit-event'],
], [], false);
add('route via', 'example', [
  ['port', 'add', 'arch.mdg', 'protected-endpoint', '--direction', 'output', '--name', 'Audit Event', '--id', 'audit-event'],
  ['port', 'add', 'arch.mdg', 'user-routes', '--direction', 'output', '--name', 'Audit Out', '--id', 'audit-out'],
  ['port', 'add', 'arch.mdg', 'api-service', '--direction', 'output', '--name', 'Audit Out', '--id', 'audit-out'],
  ['port', 'add', 'arch.mdg', 'jwt-validator', '--direction', 'input', '--name', 'Audit In', '--id', 'audit-in'],
  ['connect', 'arch.mdg', 'protected-endpoint:audit-event', 'jwt-validator:audit-in', '--route', '--via', 'user-routes:audit-out', 'api-service:audit-out'],
  ['connect', 'arch.mdg', 'protected-endpoint:audit-event', 'jwt-validator:audit-in', '--route', '--via', 'frontend:login-submitted'],
  ['connect', 'arch.mdg', 'protected-endpoint:audit-event', 'jwt-validator:audit-in', '--route', '--via', 'user-routes:profile-request'],
  ['connect', 'arch.mdg', 'frontend:login-submitted', 'frontend:session-established', '--route'],
  ['connections', 'arch.mdg', 'api-service', '--json'],
], ['arch.mdg'], false);

// ---------------------------------------------------------------- runner
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'mdg-golden-'));
const startDir = process.cwd();
for (const [i, c] of cases.entries()) {
  const dir = path.join(root, String(i));
  fs.mkdirSync(dir);
  process.chdir(dir);
  setups[c.setup]();
  c.results = [];
  for (const step of c.steps) {
    let stdout = '';
    let stderr = '';
    const code = await runCli(step.args, {
      stdout: (t) => (stdout += t),
      stderr: (t) => (stderr += t),
      readStdin: () => step.stdin ?? '',
    });
    c.results.push({ code, stdout: normalize(stdout, dir), stderr: normalize(stderr, dir) });
  }
  c.written = {};
  for (const f of c.files) {
    if (!fs.existsSync(f)) {
      c.written[f] = null;
      continue;
    }
    const data = fs.readFileSync(f);
    c.written[f] = f.endsWith('.mdg') ? data.toString('base64') : data.toString('utf8');
  }
  process.chdir(startDir);
}
fs.rmSync(root, { recursive: true, force: true });
fs.writeFileSync(out, `${JSON.stringify(cases, null, 2)}\n`);
const invocations = cases.reduce((n, c) => n + c.steps.length, 0);
console.log(`Wrote ${cases.length} cases (${invocations} invocations) to ${path.relative(process.cwd(), out)}`);
