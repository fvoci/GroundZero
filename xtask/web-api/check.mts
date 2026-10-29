// D-131 가1. Use the installed TS parser and symbols, not a search for URL strings.
import { readdirSync, readFileSync, mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import {
  capability,
  networkExit,
  canonicalPath,
  compact,
  readNetworkPolicy,
  type NetworkPolicy,
  type NetworkEntry,
  type Exit,
} from './network.mts';

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
  const outlets: string[] = [];
  const actionMethods = new Map<object, string>();
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
      if (
        node.type === 'RegularElement' &&
        node.name === 'form' &&
        Array.isArray(node.attributes)
      ) {
        const attributes: unknown[] = node.attributes;
        const action = attributes.find(
          (attribute: unknown) => isRecord(attribute) && attribute.name === 'action',
        );
        const method = attributes.find(
          (attribute: unknown) => isRecord(attribute) && attribute.name === 'method',
        );
        if (isRecord(action) && isRecord(method)) {
          const part: unknown = Array.isArray(method.value) ? method.value[0] : method.value;
          if (isRecord(part) && part.type === 'Text')
            actionMethods.set(action, JSON.stringify(String(part.data).toUpperCase()));
          else if (
            isRecord(part) &&
            isRecord(part.expression) &&
            typeof part.expression.start === 'number' &&
            typeof part.expression.end === 'number'
          )
            actionMethods.set(action, source.slice(part.expression.start, part.expression.end));
        }
        if (!action) {
          const line =
            typeof node.start === 'number' ? source.slice(0, node.start).split('\n').length : 1;
          outlets.push(`__fvoci_network('form', '', ${String(line)});`);
        }
      }
      if (
        node.type === 'Attribute' &&
        typeof node.name === 'string' &&
        ['href', 'src', 'action', 'srcdoc'].includes(node.name)
      ) {
        const parts = Array.isArray(node.value) ? node.value : [node.value];
        const expression = parts
          .map((part: unknown) => {
            const value = record(part);
            if (value.type === 'Text') return JSON.stringify(value.data);
            const span = record(value.expression);
            if (typeof span.start !== 'number' || typeof span.end !== 'number')
              throw new Error('attribute has no expression offsets');
            return `(${source.slice(span.start, span.end)})`;
          })
          .join(' + ');
        const line =
          typeof node.start === 'number' ? source.slice(0, node.start).split('\n').length : 1;
        outlets.push(
          `__fvoci_network(${JSON.stringify(node.name)}, ${expression || "''"}, ${String(line)}${actionMethods.has(node) ? `, ${actionMethods.get(node) ?? "'GET'"}` : ''});`,
        );
      }
      if (
        node.type === 'RegularElement' &&
        node.name === 'script' &&
        isRecord(node.fragment) &&
        Array.isArray(node.fragment.nodes)
      ) {
        for (const part of node.fragment.nodes)
          if (
            isRecord(part) &&
            part.type === 'Text' &&
            typeof part.start === 'number' &&
            typeof part.end === 'number'
          )
            copy(part.start, part.end);
      }
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
  return `${masked.join('')}\n${outlets.join('\n')}`;
}

function sourceProgram(files: Map<string, string>): ts.Program {
  const sources = new Map(
    [...files].map(([file, code]) => {
      const svelte = /\.(svelte|html)$/.test(file);
      const css = file.endsWith('.css');
      const path = resolve(virtualRoot, svelte || css ? `${file}.ts` : file);
      const cssSource = [
        ...code.matchAll(/url\(\s*['"]?([^)'"\s]+)['"]?\s*\)|@import\s+['"]([^'"]+)['"]/g),
      ]
        .map(
          (match) =>
            `__fvoci_network('css', ${JSON.stringify(match[1] ?? match[2])}, ${String(code.slice(0, match.index).split('\n').length)});`,
        )
        .join('\n');
      return [
        path,
        ts.createSourceFile(
          path,
          svelte ? svelteSource(code) : css ? cssSource : code,
          ts.ScriptTarget.Latest,
          true,
        ),
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

function apiPath(path: string): boolean {
  return /^\/(?:api|auth|healthz)(?:\/|$)/.test(path);
}

export function checkUsage(
  files: Map<string, string>,
  contract: unknown,
  allowlist: Allowlist,
  policy: NetworkPolicy = { websockets: [], sites: [] },
): { calls: number; used: string[]; unused: string[]; exits: NetworkEntry[] } {
  // Validate injected test inputs as strictly as the on-disk list.
  allowlist = readAllowlist(allowlist);
  policy = readNetworkPolicy(policy);
  const exits: NetworkEntry[] = [];
  const exceptionsUsed = new Set<NetworkPolicy['sites'][number]>();
  const websocketsUsed = new Set<NetworkPolicy['websockets'][number]>();
  const program = sourceProgram(files);
  const checker = program.getTypeChecker();
  const problems: string[] = [];
  const operations = operationList(contract);
  const targets = new Map<ts.Symbol, Forwarder | null>();
  const forwarded = new Map<Forwarder, number>();
  const used = new Set<string>();
  let transport: ts.FunctionDeclaration | undefined;
  let calls = 0;
  function unalias(found: ts.Symbol): ts.Symbol {
    return (found.flags & ts.SymbolFlags.Alias) !== 0 ? checker.getAliasedSymbol(found) : found;
  }
  function symbol(node: ts.Node): ts.Symbol | undefined {
    const found =
      ts.isIdentifier(node) && ts.isShorthandPropertyAssignment(node.parent)
        ? checker.getShorthandAssignmentValueSymbol(node.parent)
        : checker.getSymbolAtLocation(node);
    return found ? unalias(found) : undefined;
  }
  function parameterWrites(
    fn: ts.FunctionLikeDeclaration,
    parameters: readonly ts.ParameterDeclaration[] = fn.parameters,
  ): string[] {
    const watched = new Set(
      parameters.flatMap((parameter) => {
        const found = symbol(parameter.name);
        return found ? [found] : [];
      }),
    );
    const writes: string[] = [];
    function assigned(node: ts.Node): void {
      if (ts.isIdentifier(node)) {
        const found = symbol(node);
        if (found && watched.has(found)) writes.push(node.text);
      } else if (ts.isShorthandPropertyAssignment(node)) {
        const found = checker.getShorthandAssignmentValueSymbol(node);
        if (found && watched.has(unalias(found))) writes.push(node.name.text);
      } else if (ts.isPropertyAssignment(node)) assigned(node.initializer);
      else if (ts.isArrayLiteralExpression(node)) node.elements.forEach(assigned);
      else if (ts.isObjectLiteralExpression(node)) node.properties.forEach(assigned);
      else if (
        ts.isParenthesizedExpression(node) ||
        ts.isSpreadElement(node) ||
        ts.isSpreadAssignment(node)
      )
        assigned(node.expression);
      else if (ts.isBinaryExpression(node)) assigned(node.left); // Destructuring defaults.
    }
    if (fn.body)
      walk(fn.body, (node) => {
        if (
          ts.isBinaryExpression(node) &&
          node.operatorToken.kind >= ts.SyntaxKind.FirstAssignment &&
          node.operatorToken.kind <= ts.SyntaxKind.LastAssignment
        )
          assigned(node.left);
        if (
          (ts.isPrefixUnaryExpression(node) || ts.isPostfixUnaryExpression(node)) &&
          (node.operator === ts.SyntaxKind.PlusPlusToken ||
            node.operator === ts.SyntaxKind.MinusMinusToken)
        )
          assigned(node.operand);
        if (
          (ts.isForOfStatement(node) || ts.isForInStatement(node)) &&
          !ts.isVariableDeclarationList(node.initializer)
        )
          assigned(node.initializer);
      });
    return [...new Set(writes)];
  }
  function isFetch(node: ts.Node, seen = new Set<ts.Symbol>()): boolean {
    return capability(node, checker, symbol, seen) === 'fetch';
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
      if (path === shared && name === 'request') {
        targets.set(found, null);
        transport = node;
      }
      const entry = allowlist.forwarders.find((row) => row.file === path && row.name === name);
      if (entry) {
        targets.set(found, entry);
        forwarded.set(entry, 0);
      }
      if (targets.has(found)) {
        for (const name of parameterWrites(node, node.parameters.slice(0, 2))) {
          problems.push(`${path}:${node.name.text}: parameter ${name} is written`);
        }
      }
    });
  }
  if (![...targets.values()].includes(null)) problems.push(`missing ${shared}:request`);
  for (const row of allowlist.forwarders) {
    if (!forwarded.has(row)) problems.push(`stale forwarder: ${row.file}:${row.name}`);
  }

  type Bindings = Map<ts.Symbol, ts.Expression | string[]>;
  function values(
    node: ts.Expression | undefined,
    seen = new Set<ts.Node>(),
    bindings: Bindings = new Map(),
  ): string[] {
    if (!node || seen.has(node)) throw new Error('method/path is not statically resolvable');
    seen = new Set(seen).add(node);
    const found = symbol(node);
    const binding = found ? bindings.get(found) : undefined;
    if (binding) return Array.isArray(binding) ? binding : values(binding, seen, bindings);
    const evaluate = (expression: ts.Expression | undefined): string[] =>
      values(expression, seen, bindings);
    if (ts.isStringLiteralLike(node)) return [node.text];
    if (
      node.kind === ts.SyntaxKind.NullKeyword ||
      (checker.getTypeAtLocation(node).flags & ts.TypeFlags.Undefined) !== 0
    )
      return [''];
    if (
      [
        'window.location.host',
        'location.host',
        'window.location.origin',
        'location.origin',
      ].includes(node.getText())
    )
      return [node.getText().endsWith('origin') ? 'https://fvoci.invalid' : 'fvoci.invalid'];
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === '$derived' &&
      !symbol(node.expression)
    )
      return evaluate(node.arguments[0]);
    if (
      ts.isCallExpression(node) &&
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.getText() === 'URL.createObjectURL'
    )
      return ['blob:fvoci-generated'];
    if (
      ts.isNewExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === 'URL'
    )
      return evaluate(node.arguments?.[0]);
    if (ts.isNewExpression(node) && capability(node.expression, checker, symbol) === 'Request')
      return evaluate(node.arguments?.[0]);
    if (
      ts.isParenthesizedExpression(node) ||
      ts.isAsExpression(node) ||
      ts.isSatisfiesExpression(node)
    ) {
      return evaluate(node.expression);
    }
    if (ts.isConditionalExpression(node)) {
      return [...new Set([...evaluate(node.whenTrue), ...evaluate(node.whenFalse)])];
    }
    if (ts.isTemplateExpression(node)) {
      let prefixes = [node.head.text];
      for (const span of node.templateSpans) {
        prefixes = prefixes.flatMap((prefix) =>
          /[?#]/.test(prefix)
            ? [prefix]
            : evaluate(span.expression).map((value) => prefix + value + span.literal.text),
        );
      }
      return prefixes;
    }
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.PlusToken) {
      return evaluate(node.left).flatMap((prefix) =>
        /[?#]/.test(prefix) ? [prefix] : evaluate(node.right).map((value) => prefix + value),
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
      try {
        return evaluate(node.arguments[0]).map((value) => encodeURIComponent(value));
      } catch (error: unknown) {
        if (
          error instanceof Error &&
          /URL helper must be const|URL function parameter is written/.test(error.message)
        )
          throw error;
        /* Runtime encoded path parameters remain a single symbolic segment. */
      }
      return ['{}'];
    }
    if (ts.isCallExpression(node)) {
      const declaration = symbol(node.expression)?.valueDeclaration;
      if (
        declaration &&
        ts.isVariableDeclaration(declaration) &&
        (!ts.isVariableDeclarationList(declaration.parent) ||
          (declaration.parent.flags & ts.NodeFlags.Const) === 0)
      ) {
        throw new Error(`URL helper must be const: ${node.expression.getText()}`);
      }
      const fn =
        declaration && ts.isVariableDeclaration(declaration)
          ? declaration.initializer
          : declaration;
      if (
        fn &&
        (ts.isFunctionDeclaration(fn) || ts.isArrowFunction(fn) || ts.isFunctionExpression(fn)) &&
        fn.body
      ) {
        if (parameterWrites(fn).length > 0) throw new Error('URL function parameter is written');
        const argumentsBySymbol = new Map(bindings);
        fn.parameters.forEach((parameter, index) => {
          const found = symbol(parameter.name);
          const argument = node.arguments[index] ?? parameter.initializer;
          if (found && argument) argumentsBySymbol.set(found, argument);
        });
        const returned: ts.Expression[] = [];
        function returns(part: ts.Node): void {
          if (ts.isReturnStatement(part) && part.expression) returned.push(part.expression);
          else if (!ts.isFunctionLike(part)) ts.forEachChild(part, returns);
        }
        if (ts.isBlock(fn.body)) returns(fn.body);
        else returned.push(fn.body);
        if (returned.length > 0)
          return [
            ...new Set(
              returned.flatMap((expression) => values(expression, seen, argumentsBySymbol)),
            ),
          ];
      }
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
      return evaluate(declaration.initializer);
    }
    const type = checker.getTypeAtLocation(node);
    const variants = type.isUnion() ? type.types : [type];
    if (variants.every((variant) => variant.isStringLiteral())) {
      return variants.flatMap((variant) => (variant.isStringLiteral() ? [variant.value] : []));
    }
    throw new Error(`method/path is not statically resolvable: ${node.getText()}`);
  }

  // Undefined denotes an absent method property, so a spread without method does
  // not overwrite an earlier method. An explicit method: undefined means GET.
  function isUndefined(node: ts.Expression): boolean {
    return (checker.getTypeAtLocation(node).flags & ts.TypeFlags.Undefined) !== 0;
  }
  function fetchMethods(
    node: ts.Expression | undefined,
    bindings: Bindings = new Map(),
    seen = new Set<ts.Node>(),
  ): (string | undefined)[] {
    if (!node || isUndefined(node)) return [undefined];
    if (seen.has(node)) throw new Error('fetch options are not statically resolvable');
    seen = new Set(seen).add(node);
    if (
      ts.isParenthesizedExpression(node) ||
      ts.isAsExpression(node) ||
      ts.isSatisfiesExpression(node)
    )
      return fetchMethods(node.expression, bindings, seen);
    if (ts.isConditionalExpression(node))
      return [
        ...fetchMethods(node.whenTrue, bindings, seen),
        ...fetchMethods(node.whenFalse, bindings, seen),
      ];
    const declaration = symbol(node)?.valueDeclaration;
    if (
      declaration &&
      ts.isVariableDeclaration(declaration) &&
      declaration.initializer &&
      ts.isVariableDeclarationList(declaration.parent) &&
      (declaration.parent.flags & ts.NodeFlags.Const) !== 0
    ) {
      const optionSymbol = symbol(declaration.name);
      walk(declaration.getSourceFile(), (part) => {
        if (
          !ts.isBinaryExpression(part) ||
          part.operatorToken.kind < ts.SyntaxKind.FirstAssignment ||
          part.operatorToken.kind > ts.SyntaxKind.LastAssignment
        )
          return;
        walk(part.left, (member) => {
          if (
            (ts.isPropertyAccessExpression(member) || ts.isElementAccessExpression(member)) &&
            symbol(member.expression) === optionSymbol &&
            (ts.isElementAccessExpression(member) || member.name.text === 'method')
          )
            throw new Error('fetch options.method is written');
        });
      });
      return fetchMethods(declaration.initializer, bindings, seen);
    }
    if (!ts.isObjectLiteralExpression(node))
      throw new Error(`fetch options are not statically resolvable: ${node.getText()}`);
    let verbs: (string | undefined)[] = [undefined];
    for (const property of node.properties) {
      if (ts.isSpreadAssignment(property)) {
        const spread = fetchMethods(property.expression, bindings, seen);
        verbs = verbs.flatMap((verb) => spread.map((value) => value ?? verb));
        continue;
      }
      const names = ts.isComputedPropertyName(property.name)
        ? values(property.name.expression, new Set(), bindings)
        : [property.name.getText().replace(/^['"]|['"]$/g, '')];
      if (!names.includes('method')) continue;
      const expression = ts.isPropertyAssignment(property)
        ? property.initializer
        : ts.isShorthandPropertyAssignment(property)
          ? property.name
          : undefined;
      if (!expression) throw new Error('fetch method is not statically resolvable');
      const value = isUndefined(expression) ? ['GET'] : values(expression, new Set(), bindings);
      verbs = names.some((name) => name !== 'method') ? [...verbs, ...value] : value;
    }
    return [...new Set(verbs)];
  }

  function filename(file: ts.SourceFile): string {
    const path = relative(virtualRoot, file.fileName);
    return files.has(path) ? path : path.replace(/\.(svelte|html|css)\.ts$/, '.$1');
  }
  function inspect(exit: Exit, node: ts.Node, file: ts.SourceFile, required = false): void {
    const row: NetworkEntry = {
      file: filename(file),
      line: exit.line ?? file.getLineAndCharacterOfPosition(node.getStart()).line + 1,
      kind: exit.kind,
      expression: compact(exit.url?.getText() ?? ''),
      classification: '대조함',
      reason: '',
      operations: [],
    };
    exits.push(row);
    const location = `${row.file}:${String(row.line)}`;
    const site = policy.sites.find(
      (site) =>
        site.file === row.file &&
        site.kind === row.kind &&
        compact(site.expression) === row.expression,
    );
    const except = (reason: string): void => {
      row.classification = '이유 있는 예외';
      row.reason = reason;
    };
    try {
      if (exit.kind === 'location.reload') {
        except('현재 문서를 다시 읽으며 새 URL을 구성하지 않는다.');
        return;
      }
      if (exit.kind === 'XMLHttpRequest constructor') {
        except('생성자는 URL이 없고 open(method, url)에서 대조한다.');
        return;
      }
      if (exit.kind === 'form') {
        except(
          '명시 action 없는 현재 앱 화면의 폼이다. 제출 처리는 앱 이벤트 핸들러에 있으며 새 계약 URL을 구성하지 않는다.',
        );
        return;
      }
      if (
        [
          'srcdoc',
          'innerHTML',
          'outerHTML',
          'dynamic attribute',
          'Worker',
          'SharedWorker',
          'WASM loader',
        ].includes(exit.kind)
      ) {
        if (!site) throw new Error('network exit requires a reasoned exception');
        exceptionsUsed.add(site);
        except(site.reason);
        return;
      }
      if (
        (exit.kind === 'import' || exit.kind === 'css') &&
        exit.url &&
        ts.isStringLiteralLike(exit.url) &&
        !apiPath(exit.url.text)
      ) {
        if (/^(?:\/|[a-z][\w+.-]*:)/i.test(exit.url.text)) canonicalPath(exit.url.text);
        except('Vite가 해석하는 패키지·상대 경로 자원이며 서버 API 주소가 아니다.');
        return;
      }
      let paths: string[];
      try {
        paths = values(exit.url);
      } catch (error: unknown) {
        if (!site || exit.kind === 'fetch' || exit.kind === 'Request') throw error;
        exceptionsUsed.add(site);
        except(site.reason);
        return;
      }
      let verbs = exit.verb
        ? [exit.verb]
        : exit.method
          ? values(exit.method).map((method) => method.toUpperCase())
          : fetchMethods(exit.options).map((verb) => verb ?? 'GET');
      // Fetch inherits a Request's URL/method unless explicit options override it.
      if (
        exit.kind === 'fetch' &&
        exit.url &&
        ts.isNewExpression(exit.url) &&
        capability(exit.url.expression, checker, symbol) === 'Request'
      ) {
        paths = values(exit.url.arguments?.[0]);
        const overrides = fetchMethods(exit.options);
        verbs = overrides.flatMap((verb) =>
          verb
            ? [verb]
            : fetchMethods(
                exit.url && ts.isNewExpression(exit.url) ? exit.url.arguments?.[1] : undefined,
              ).map((value) => value ?? 'GET'),
        );
      }
      if (exit.append)
        paths = paths.flatMap((path) => values(exit.append).map((room) => `${path}/${room}`));
      for (const raw of [...new Set(paths)]) {
        canonicalPath(raw);
        const absolute = /^(?:[a-z][\w+.-]*:|\/\/)/i.test(raw);
        const socket = exit.kind === 'WebSocket';
        const path =
          socket || /^https?:\/\/fvoci\.invalid(?:\/|$)/.test(raw)
            ? raw.replace(/^(?:https?|wss?):\/\/[^/]+/, '')
            : raw;
        const pathname = path.split(/[?#]/, 1)[0] ?? '';
        if (socket) {
          const match = policy.websockets.find((entry) => matches(pathname, entry.path));
          if (!match) throw new Error(`unlisted WebSocket exception: GET ${pathname}`);
          websocketsUsed.add(match);
          except(match.reason);
          continue;
        }
        if (!required && ((absolute && path === raw) || !apiPath(pathname))) {
          if (!absolute && raw !== '' && !raw.startsWith('/') && exit.kind === 'fetch')
            throw new Error('relative fetch URL is not statically resolvable');
          except(
            absolute
              ? '외부 URL·blob·data 자원이며 OpenAPI의 동일 원점 HTTP 작업이 아니다.'
              : '앱 화면·정적 자원으로 이동하며 OpenAPI 작업이 아니다.',
          );
          continue;
        }
        for (const verb of [...new Set(verbs)]) {
          const matched = operations.filter(
            (operation) =>
              operation.startsWith(`${verb} `) &&
              matches(pathname, operation.slice(verb.length + 1)),
          );
          if (matched.length === 0) throw new Error(`${verb} ${pathname} not in OpenAPI`);
          const operation = matched.sort((a, b) => a.split('{').length - b.split('{').length)[0];
          if (operation) {
            used.add(operation);
            row.operations.push(operation);
          }
        }
      }
      if (row.operations.length > 0) {
        row.classification = '대조함';
        row.reason = 'OpenAPI의 경로·메서드와 대조했다.';
        calls++;
      }
    } catch (error: unknown) {
      problems.push(
        `${location}: ${error instanceof Error ? error.message : String(error)} [${row.kind}: ${row.expression}]`,
      );
    }
  }

  for (const file of sources) {
    for (const diagnostic of program.getSyntacticDiagnostics(file)) {
      problems.push(
        `${file.fileName}: ${ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')}`,
      );
    }
    walk(file, (node) => {
      if (
        ts.isParameter(node) &&
        node.type &&
        ts.isTypeQueryNode(node.type) &&
        isFetch(node.type.exprName) &&
        (!node.initializer || !isFetch(node.name))
      )
        problems.push(
          `${filename(file)}: fetch reference is not statically resolvable: parameter ${node.name.getText()}`,
        );
      if (
        (ts.isIdentifier(node) ||
          ts.isPropertyAccessExpression(node) ||
          ts.isElementAccessExpression(node)) &&
        [
          'fetch',
          'Request',
          'XMLHttpRequest',
          'XMLHttpRequest.open',
          'EventSource',
          'WebSocket',
          'WebsocketProvider',
          'Worker',
          'SharedWorker',
          'sendBeacon',
          'window.open',
          'location.assign',
          'location.replace',
          'location.reload',
          'xhr',
        ].includes(capability(node, checker, symbol) ?? '')
      ) {
        const parent = node.parent;
        let type = false;
        for (
          let ancestor = parent;
          !ts.isSourceFile(ancestor) && !ts.isStatement(ancestor);
          ancestor = ancestor.parent
        ) {
          if (ts.isTypeNode(ancestor)) {
            type = true;
            break;
          }
        }
        const property = ts.isPropertyAccessExpression(parent) && parent.name === node;
        const objectMember =
          capability(node, checker, symbol) === 'xhr' &&
          (ts.isPropertyAccessExpression(parent) || ts.isElementAccessExpression(parent)) &&
          parent.expression === node;
        const call =
          (ts.isCallExpression(parent) || ts.isNewExpression(parent)) && parent.expression === node;
        const alias =
          ts.isVariableDeclaration(parent) &&
          ts.isVariableDeclarationList(parent.parent) &&
          (parent.parent.flags & ts.NodeFlags.Const) !== 0;
        const parameter = ts.isParameter(parent);
        const binding =
          ts.isBindingElement(parent) &&
          ts.isObjectBindingPattern(parent.parent) &&
          ts.isVariableDeclaration(parent.parent.parent) &&
          ts.isVariableDeclarationList(parent.parent.parent.parent) &&
          (parent.parent.parent.parent.flags & ts.NodeFlags.Const) !== 0;
        const recipient = ts.isCallExpression(parent)
          ? symbol(parent.expression)?.valueDeclaration
          : undefined;
        const recipientParameter =
          recipient && ts.isFunctionDeclaration(recipient) && ts.isCallExpression(parent)
            ? recipient.parameters[parent.arguments.indexOf(node)]
            : undefined;
        const argument = recipientParameter ? isFetch(recipientParameter.name) : false;
        if (
          !type &&
          !property &&
          !objectMember &&
          !ts.isImportSpecifier(parent) &&
          !ts.isExportSpecifier(parent) &&
          !call &&
          !alias &&
          !parameter &&
          !binding &&
          !argument &&
          !ts.isParenthesizedExpression(parent)
        ) {
          problems.push(
            `${filename(file)}: ${isFetch(node) ? 'fetch reference' : 'network reference'} is not statically resolvable: ${node.getText()}`,
          );
        }
        if (
          parameter &&
          parent.name === node &&
          (ts.isFunctionDeclaration(parent.parent) ||
            ts.isArrowFunction(parent.parent) ||
            ts.isFunctionExpression(parent.parent) ||
            ts.isMethodDeclaration(parent.parent))
        ) {
          for (const name of parameterWrites(parent.parent, [parent]))
            problems.push(`${filename(file)}: fetch parameter ${name} is written`);
        }
      }
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
      const exit = networkExit(node, checker, symbol);
      if (!ts.isCallExpression(node)) {
        if (exit) inspect(exit, node, file);
        return;
      }
      const called = target(node.expression);
      const callee = symbol(node.expression)?.valueDeclaration;
      if (
        callee &&
        (ts.isFunctionDeclaration(callee) ||
          ts.isFunctionExpression(callee) ||
          ts.isArrowFunction(callee))
      ) {
        callee.parameters.forEach((parameter, index) => {
          const argument = node.arguments[index];
          if (argument && isFetch(parameter.name) && !isFetch(argument))
            problems.push(
              `${filename(file)}: fetch reference is not statically resolvable: override ${parameter.name.getText()}`,
            );
        });
      }
      const directFetch = isFetch(node.expression);
      if (!called && !exit) return;
      const position = file.getLineAndCharacterOfPosition(node.getStart());
      const location = `${relative(virtualRoot, file.fileName).replace(/\.svelte\.ts$/, '.svelte')}:${String(position.line + 1)}:${String(position.character + 1)}`;
      const fn = enclosingFunction(node);
      const ownerSymbol = fn?.name ? symbol(fn.name) : undefined;
      const owner = ownerSymbol ? targets.get(ownerSymbol) : undefined;
      if (
        called &&
        !directFetch &&
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
        exits.push({
          file: filename(file),
          line: position.line + 1,
          kind: 'forwarder',
          expression: compact(node.getText()),
          classification: '대조함',
          reason: '매개변수 쓰기 없이 전달하며 도우미 호출 지점에서 대조한다.',
          operations: [],
        });
        return;
      }
      try {
        // The single shared transport forwards the checked request's parameters.
        // Validate its URL and method mapping instead of counting it twice.
        if (
          directFetch &&
          fn === transport &&
          fn?.parameters[0] &&
          fn.parameters[1] &&
          node.arguments[0] &&
          symbol(node.arguments[0]) === symbol(fn.parameters[1].name)
        ) {
          const method = symbol(fn.parameters[0].name);
          const bindings: Bindings = new Map();
          if (method) bindings.set(method, ['forwarded-method']);
          const verbs = fetchMethods(node.arguments[1], bindings);
          if (verbs.length !== 1 || verbs[0] !== 'forwarded-method')
            throw new Error('request transport method is not forwarded');
          exits.push({
            file: filename(file),
            line: position.line + 1,
            kind: 'fetch transport',
            expression: compact(node.getText()),
            classification: '대조함',
            reason: '공통 request의 URL·메서드 전달을 확인했고 호출 지점에서 대조한다.',
            operations: [],
          });
          return;
        }
        if (called)
          inspect(
            { kind: 'request', url: node.arguments[1], method: node.arguments[0] },
            node,
            file,
            true,
          );
        else if (exit) inspect(exit, node, file);
      } catch (error: unknown) {
        problems.push(`${location}: ${error instanceof Error ? error.message : String(error)}`);
      }
    });
  }
  for (const [row, count] of forwarded) {
    if (count !== 1) problems.push(`forwarder must delegate exactly once: ${row.file}:${row.name}`);
  }
  for (const site of policy.sites)
    if (!exceptionsUsed.has(site))
      problems.push(`stale network site exception: ${site.file} ${site.kind} ${site.expression}`);
  for (const socket of policy.websockets)
    if (!websocketsUsed.has(socket)) problems.push(`stale WebSocket exception: ${socket.path}`);
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
  return { calls, used: [...used].sort(), unused, exits };
}

export function readSources(root: string): Map<string, string> {
  const files = new Map<string, string>();
  function visit(directory: string): void {
    for (const entry of readdirSync(resolve(root, directory), { withFileTypes: true })) {
      const path = `${directory}/${entry.name}`;
      if (path === 'apps/web/src/lib/paraglide') continue; // Generated translations, not application code.
      if (entry.isDirectory()) visit(path);
      else if (/\.(?:[cm]?[jt]sx?|svelte|css)$/.test(path) && !/\.d\.[cm]?ts$/.test(path)) {
        files.set(path, readFileSync(resolve(root, path), 'utf8'));
      }
    }
  }
  visit('apps/web/src');
  if (existsSync(resolve(root, 'apps/web/index.html')))
    files.set('apps/web/index.html', readFileSync(resolve(root, 'apps/web/index.html'), 'utf8'));
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
    const policy: unknown = JSON.parse(
      readFileSync(resolve(root, 'xtask/web-api/network-exceptions.json'), 'utf8'),
    );
    const result = checkUsage(
      readSources(root),
      contract,
      readAllowlist(list),
      readNetworkPolicy(policy),
    );
    mkdirSync(resolve(root, 'target/web-api'), { recursive: true });
    writeFileSync(
      resolve(root, 'target/web-api/network-exits.json'),
      JSON.stringify(result.exits, null, 2) + '\n',
    );
    console.log(
      `${String(result.calls)} web calls, ${String(result.used.length)} used operations, ${String(result.unused.length)} reasoned unused operations`,
    );
    console.log(
      `${String(result.exits.length)} network exits classified; target/web-api/network-exits.json`,
    );
  } catch (error: unknown) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
