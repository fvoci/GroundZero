// D-131 가1. Use the installed TS parser and symbols, not a search for URL strings.
import { readdirSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

const loadSvelte: (id: string) => typeof import('svelte/compiler') = createRequire(
  new URL('../../apps/web/package.json', import.meta.url),
);
const { parse } = loadSvelte('svelte/compiler');
const shared = 'apps/web/src/lib/shared/api/index.ts';
const virtualRoot = '/web-api-check';
const methods = new Set(['GET', 'PUT', 'POST', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS', 'TRACE']);
const reasons = new Set(['샴 가3~가5 조립 예정', '가6 판정 예정']);

export interface Forwarder {
  file: string;
  name: string;
  reason: string;
  detail: string;
}
export interface Unused {
  method: string;
  path: string;
  reason: string;
}
export interface Allowlist {
  forwarders: Forwarder[];
  unused: Unused[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function record(value: unknown): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error('expected a JSON object');
  }
  return value;
}

function text(value: unknown): string {
  if (typeof value !== 'string' || value.trim() === '') throw new Error('expected nonempty text');
  return value;
}

export function readAllowlist(value: unknown): Allowlist {
  const input = record(value);
  if (!Array.isArray(input.forwarders) || !Array.isArray(input.unused)) {
    throw new Error('allowlist needs forwarders and unused arrays');
  }
  const reason = (value: unknown): string => {
    const label = text(value);
    if (!reasons.has(label)) throw new Error(`invalid reason: ${label}`);
    return label;
  };
  return {
    forwarders: input.forwarders.map((value: unknown) => {
      const row = record(value);
      return {
        file: text(row.file),
        name: text(row.name),
        reason: reason(row.reason),
        detail: text(row.detail),
      };
    }),
    unused: input.unused.map((value: unknown) => {
      const row = record(value);
      return { method: text(row.method), path: text(row.path), reason: reason(row.reason) };
    }),
  };
}

// Keep original offsets for diagnostics, including calls in Svelte markup. Script
// bodies and markup expressions become one TS module so imports retain identity.
function svelteSource(source: string): string {
  const ast = parse(source, { modern: true });
  // Offsets in both parsers are UTF-16, not Unicode code points.
  const masked: string[] = source.split('').map((ch) => (ch === '\n' || ch === '\r' ? ch : ' '));
  function copy(start: number, end: number): void {
    for (let index = start; index < end; index++) masked[index] = source[index] ?? ' ';
  }
  for (const script of [ast.instance, ast.module]) {
    if (script) {
      const span = record(script.content);
      if (typeof span.start !== 'number' || typeof span.end !== 'number') {
        throw new Error('Svelte script has no source offsets');
      }
      copy(span.start, span.end);
    }
  }
  function visit(value: unknown): void {
    if (Array.isArray(value)) {
      value.forEach(visit);
    } else if (typeof value === 'object' && value !== null) {
      const node = record(value);
      const declaration = node.type === 'VariableDeclaration';
      const expression =
        typeof node.type === 'string' &&
        (node.type.endsWith('Expression') ||
          ['Identifier', 'Literal', 'TemplateLiteral'].includes(node.type));
      if (
        (declaration || expression) &&
        typeof node.start === 'number' &&
        typeof node.end === 'number'
      ) {
        copy(node.start, node.end);
        masked[node.start - 1] = declaration ? ';' : '(';
        masked[node.end] = declaration ? ';' : ')';
        if (masked[node.start - 2] === ' ') masked[node.start - 2] = ';';
        if (masked[node.end + 1] === ' ') masked[node.end + 1] = ';';
        return; // Do not re-copy child JS nodes over the same expression.
      }
      Object.values(node).forEach(visit);
    }
  }
  visit(ast.fragment);
  return masked.join('');
}

function sourceProgram(files: Map<string, string>): ts.Program {
  const sources = new Map(
    [...files].map(([file, code]) => {
      const svelte = file.endsWith('.svelte');
      const path = resolve(virtualRoot, svelte ? `${file}.ts` : file);
      return [
        path,
        ts.createSourceFile(path, svelte ? svelteSource(code) : code, ts.ScriptTarget.Latest, true),
      ];
    }),
  );
  const options: ts.CompilerOptions = {
    noLib: true,
    allowJs: true,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    paths: {
      '$lib/*': [`${virtualRoot}/apps/web/src/lib/*`],
      '@fvoci/api-client': [`${virtualRoot}/packages/api-client/src/index.ts`],
    },
  };
  const host = ts.createCompilerHost(options);
  host.getSourceFile = (file) => sources.get(file);
  host.fileExists = (file) => sources.has(file);
  host.readFile = (file) => sources.get(file)?.text;
  host.directoryExists = (dir) => [...sources.keys()].some((file) => file.startsWith(`${dir}/`));
  host.getCurrentDirectory = () => virtualRoot;
  return ts.createProgram([...sources.keys()], options, host);
}

function walk(node: ts.Node, visit: (node: ts.Node) => void): void {
  visit(node);
  ts.forEachChild(node, (child) => {
    walk(child, visit);
  });
}

function enclosingFunction(node: ts.Node): ts.FunctionDeclaration | undefined {
  for (let parent = node.parent; !ts.isSourceFile(parent); parent = parent.parent) {
    if (ts.isFunctionDeclaration(parent)) return parent;
    if (ts.isFunctionLike(parent)) return undefined;
  }
  return undefined;
}

function operationList(contract: unknown): string[] {
  return Object.entries(record(record(contract).paths)).flatMap(([path, item]) =>
    Object.keys(record(item))
      .filter((method) => methods.has(method.toUpperCase()))
      .map((method) => `${method.toUpperCase()} ${path}`),
  );
}

function matches(path: string, route: string): boolean {
  const actual = path.split('/');
  const expected = route.split('/');
  return (
    actual.length === expected.length &&
    expected.every((part, index) => {
      const value = actual[index];
      return /^\{[^/{}]+\}$/.test(part) ? value !== undefined && value !== '' : value === part;
    })
  );
}

export function checkUsage(
  files: Map<string, string>,
  contract: unknown,
  allowlist: Allowlist,
): { calls: number; used: string[]; unused: string[] } {
  // Validate injected test inputs as strictly as the on-disk list.
  allowlist = readAllowlist(allowlist);
  const program = sourceProgram(files);
  const checker = program.getTypeChecker();
  const problems: string[] = [];
  const operations = operationList(contract);
  const targets = new Map<ts.Symbol, Forwarder | null>();
  const forwarded = new Map<Forwarder, number>();
  const used = new Set<string>();
  let calls = 0;
  function unalias(found: ts.Symbol): ts.Symbol {
    return (found.flags & ts.SymbolFlags.Alias) !== 0 ? checker.getAliasedSymbol(found) : found;
  }
  function symbol(node: ts.Node): ts.Symbol | undefined {
    const found = checker.getSymbolAtLocation(node);
    return found ? unalias(found) : undefined;
  }
  function target(node: ts.Node, seen = new Set<ts.Symbol>()): ts.Symbol | undefined {
    if (ts.isParenthesizedExpression(node)) return target(node.expression, seen);
    const found = symbol(node);
    const declaration = found?.valueDeclaration;
    if (
      ts.isElementAccessExpression(node) ||
      (declaration &&
        (ts.isBindingElement(declaration) || ts.isShorthandPropertyAssignment(declaration)))
    ) {
      for (const signature of checker.getTypeAtLocation(node).getCallSignatures()) {
        const fn = signature.getDeclaration();
        const origin = ts.isFunctionDeclaration(fn) && fn.name ? symbol(fn.name) : undefined;
        if (origin && targets.has(origin)) return origin;
      }
    }
    if (!found || seen.has(found)) return undefined;
    if (targets.has(found)) return found;
    seen.add(found);
    if (
      declaration &&
      ts.isVariableDeclaration(declaration) &&
      declaration.initializer &&
      ts.isVariableDeclarationList(declaration.parent) &&
      (declaration.parent.flags & ts.NodeFlags.Const) !== 0
    ) {
      return target(declaration.initializer, seen);
    }
    return undefined;
  }
  const sources = program.getSourceFiles();
  for (const file of sources) {
    walk(file, (node) => {
      if (!ts.isFunctionDeclaration(node) || !node.name) return;
      const path = relative(virtualRoot, file.fileName);
      const name = node.name.text;
      const found = symbol(node.name);
      if (!found) return;
      if (path === shared && name === 'request') targets.set(found, null);
      const entry = allowlist.forwarders.find((row) => row.file === path && row.name === name);
      if (entry) {
        targets.set(found, entry);
        forwarded.set(entry, 0);
      }
    });
  }
  if (![...targets.values()].includes(null)) problems.push(`missing ${shared}:request`);
  for (const row of allowlist.forwarders) {
    if (!forwarded.has(row)) problems.push(`stale forwarder: ${row.file}:${row.name}`);
  }

  function values(node: ts.Expression | undefined, seen = new Set<ts.Node>()): string[] {
    if (!node || seen.has(node)) throw new Error('method/path is not statically resolvable');
    seen = new Set(seen).add(node);
    if (ts.isStringLiteralLike(node)) return [node.text];
    if (
      ts.isParenthesizedExpression(node) ||
      ts.isAsExpression(node) ||
      ts.isSatisfiesExpression(node)
    ) {
      return values(node.expression, seen);
    }
    if (ts.isConditionalExpression(node)) {
      return [...new Set([...values(node.whenTrue, seen), ...values(node.whenFalse, seen)])];
    }
    if (ts.isTemplateExpression(node)) {
      let prefixes = [node.head.text];
      for (const span of node.templateSpans) {
        prefixes = prefixes.flatMap((prefix) =>
          /[?#]/.test(prefix)
            ? [prefix]
            : values(span.expression, seen).map((value) => prefix + value + span.literal.text),
        );
      }
      return prefixes;
    }
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.PlusToken) {
      return values(node.left, seen).flatMap((prefix) =>
        /[?#]/.test(prefix) ? [prefix] : values(node.right, seen).map((value) => prefix + value),
      );
    }
    // Only encoded values may stand for an arbitrary path segment. An unknown
    // resource name or a suffix that might add slashes must fail closed.
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === 'encodeURIComponent' &&
      !symbol(node.expression)
    ) {
      return ['{}'];
    }
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === 'String' &&
      !symbol(node.expression) &&
      node.arguments.length === 1 &&
      node.arguments[0] &&
      (checker.getTypeAtLocation(node.arguments[0]).flags & ts.TypeFlags.NumberLike) !== 0
    ) {
      return ['{}'];
    }
    const declaration = symbol(node)?.valueDeclaration;
    if (
      declaration &&
      ts.isVariableDeclaration(declaration) &&
      declaration.initializer &&
      ts.isVariableDeclarationList(declaration.parent) &&
      (declaration.parent.flags & ts.NodeFlags.Const) !== 0
    ) {
      return values(declaration.initializer, seen);
    }
    const type = checker.getTypeAtLocation(node);
    const variants = type.isUnion() ? type.types : [type];
    if (variants.every((variant) => variant.isStringLiteral())) {
      return variants.flatMap((variant) => (variant.isStringLiteral() ? [variant.value] : []));
    }
    throw new Error(`method/path is not statically resolvable: ${node.getText()}`);
  }

  for (const file of sources) {
    for (const diagnostic of program.getSyntacticDiagnostics(file)) {
      problems.push(
        `${file.fileName}: ${ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')}`,
      );
    }
    walk(file, (node) => {
      if (ts.isIdentifier(node)) {
        const found = symbol(node);
        const namespace =
          found?.exports &&
          checker.getExportsOfModule(found).some((entry) => targets.has(unalias(entry)));
        const parent = node.parent;
        const member =
          (ts.isPropertyAccessExpression(parent) || ts.isElementAccessExpression(parent)) &&
          parent.expression === node;
        if (namespace && !member && !ts.isNamespaceImport(parent)) {
          problems.push(
            `${relative(virtualRoot, file.fileName)}: API namespace reference is not statically resolvable: ${node.getText()}`,
          );
        }
      }
      if (
        ts.isElementAccessExpression(node) &&
        !ts.isStringLiteralLike(node.argumentExpression) &&
        !target(node)
      ) {
        const apiMembers = checker
          .getTypeAtLocation(node.expression)
          .getProperties()
          .some((entry) => targets.has(unalias(entry)));
        if (apiMembers)
          problems.push(
            `${relative(virtualRoot, file.fileName)}: API member access is not statically resolvable: ${node.getText()}`,
          );
      }
      // A callback/assignment could hide calls from the scanner. Only direct calls,
      // imports/reexports and immutable aliases are supported; reject other uses.
      if (
        (ts.isIdentifier(node) ||
          ts.isPropertyAccessExpression(node) ||
          ts.isElementAccessExpression(node)) &&
        target(node)
      ) {
        const parent = node.parent;
        const declaration = ts.isFunctionDeclaration(parent) && parent.name === node;
        const importExport = ts.isImportSpecifier(parent) || ts.isExportSpecifier(parent);
        const propertyName = ts.isPropertyAccessExpression(parent) && parent.name === node;
        const callee = ts.isCallExpression(parent) && parent.expression === node;
        const alias =
          ts.isVariableDeclaration(parent) &&
          ts.isVariableDeclarationList(parent.parent) &&
          (parent.parent.flags & ts.NodeFlags.Const) !== 0;
        const parentheses = ts.isParenthesizedExpression(parent);
        if (!declaration && !importExport && !propertyName && !callee && !alias && !parentheses) {
          const position = file.getLineAndCharacterOfPosition(node.getStart());
          problems.push(
            `${relative(virtualRoot, file.fileName)}:${String(position.line + 1)}: API helper reference is not statically resolvable: ${node.getText()}`,
          );
        }
      }
      if (!ts.isCallExpression(node)) return;
      const called = target(node.expression);
      if (!called) return;
      const position = file.getLineAndCharacterOfPosition(node.getStart());
      const location = `${relative(virtualRoot, file.fileName).replace(/\.svelte\.ts$/, '.svelte')}:${String(position.line + 1)}:${String(position.character + 1)}`;
      const fn = enclosingFunction(node);
      const ownerSymbol = fn?.name ? symbol(fn.name) : undefined;
      const owner = ownerSymbol ? targets.get(ownerSymbol) : undefined;
      if (
        owner &&
        fn &&
        node.arguments[0] &&
        node.arguments[1] &&
        fn.parameters[0] &&
        fn.parameters[1] &&
        symbol(node.arguments[0]) === symbol(fn.parameters[0].name) &&
        symbol(node.arguments[1]) === symbol(fn.parameters[1].name)
      ) {
        forwarded.set(owner, (forwarded.get(owner) ?? 0) + 1);
        return;
      }
      calls++;
      try {
        const verbs = values(node.arguments[0]);
        const paths = values(node.arguments[1]).map((path) => path.split(/[?#]/, 1)[0] ?? '');
        for (const verb of verbs) {
          for (const path of paths) {
            const matched = operations.filter(
              (operation) =>
                operation.startsWith(`${verb} `) && matches(path, operation.slice(verb.length + 1)),
            );
            if (matched.length === 0) problems.push(`${location}: ${verb} ${path} not in OpenAPI`);
            // Prefer a static route over a parameter route, as the server does.
            else
              used.add(matched.sort((a, b) => a.split('{').length - b.split('{').length)[0] ?? '');
          }
        }
      } catch (error: unknown) {
        problems.push(`${location}: ${error instanceof Error ? error.message : String(error)}`);
      }
    });
  }
  for (const [row, count] of forwarded) {
    if (count !== 1) problems.push(`forwarder must delegate exactly once: ${row.file}:${row.name}`);
  }
  const unused = operations.filter((operation) => !used.has(operation)).sort();
  const listed = new Set<string>();
  for (const row of allowlist.unused) {
    const operation = `${row.method} ${row.path}`;
    if (listed.has(operation)) problems.push(`duplicate unused entry: ${operation}`);
    listed.add(operation);
    if (!unused.includes(operation)) problems.push(`stale unused entry: ${operation}`);
  }
  for (const operation of unused) {
    if (!listed.has(operation)) problems.push(`unlisted unused operation: ${operation}`);
  }
  if (problems.length > 0) throw new Error(problems.join('\n'));
  return { calls, used: [...used].sort(), unused };
}

export function readSources(root: string): Map<string, string> {
  const files = new Map<string, string>();
  function visit(directory: string): void {
    for (const entry of readdirSync(resolve(root, directory), { withFileTypes: true })) {
      const path = `${directory}/${entry.name}`;
      if (path === 'apps/web/src/lib/paraglide') continue; // Generated translations, not application code.
      if (entry.isDirectory()) visit(path);
      else if (/\.(?:[cm]?[jt]sx?|svelte)$/.test(path) && !/\.d\.[cm]?ts$/.test(path)) {
        files.set(path, readFileSync(resolve(root, path), 'utf8'));
      }
    }
  }
  visit('apps/web/src');
  // Generated literal unions (for example MarkTarget) participate in resolution,
  // but only web sources contain the request call sites under inspection.
  for (const path of [
    'packages/api-client/src/index.ts',
    'packages/api-client/src/generated/schema.ts',
  ]) {
    files.set(path, readFileSync(resolve(root, path), 'utf8'));
  }
  return files;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const root = resolve(process.argv[2] ?? '.');
    const contract: unknown = JSON.parse(
      readFileSync(resolve(root, 'contracts/openapi.json'), 'utf8'),
    );
    const list: unknown = JSON.parse(
      readFileSync(resolve(root, 'xtask/web-api/allowlist.json'), 'utf8'),
    );
    const result = checkUsage(readSources(root), contract, readAllowlist(list));
    console.log(
      `${String(result.calls)} web calls, ${String(result.used.length)} used operations, ${String(result.unused.length)} reasoned unused operations`,
    );
  } catch (error: unknown) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
