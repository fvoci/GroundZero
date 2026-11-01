// D-131 가1. Use the installed TS parser and symbols, not a search for URL strings.
import { readdirSync, readFileSync, existsSync } from 'node:fs';
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
const designReason = '설계상 직접 호출하지 않음';
const reasons = new Set(['샴 가3~가5 조립 예정', designReason]);

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
  detail?: string;
}
export interface Allowlist {
  forwarders: Forwarder[];
  unused: Unused[];
  websocket?: { name: string; path: string; reason: string };
}

interface Exit {
  kind: 'fetch' | 'link' | 'location' | 'WebSocket' | 'request';
  url?: ts.Expression | undefined;
  options?: ts.Expression | undefined;
  method?: ts.Expression | undefined;
  line?: number | undefined;
  append?: ts.Expression | undefined;
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
  const websocket = input.websocket === undefined ? undefined : record(input.websocket);
  return {
    ...(websocket
      ? {
          websocket: {
            name: text(websocket.name),
            path: text(websocket.path),
            reason: text(websocket.reason),
          },
        }
      : {}),
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
      const label = reason(row.reason);
      return {
        method: text(row.method),
        path: text(row.path),
        reason: label,
        ...(label === designReason ? { detail: text(row.detail) } : {}),
      };
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
      if (node.type === 'Attribute' && typeof node.name === 'string' && node.name === 'href') {
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
        outlets.push(`__fvoci_link(${expression || "''"}, ${String(line)});`);
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

function canonicalPath(url: string): void {
  const path = (url.split(/[?#]/, 1)[0] ?? '').replace(/%2e/gi, '.');
  if (/(?:^|\/)\.{1,2}(?:\/|$)/.test(path) || /\\|%2f|%5c/i.test(path))
    throw new Error('noncanonical URL or dot segment: ' + url);
}

function apiPath(path: string): boolean {
  return /^\/(?:api|auth|healthz)(?:\/|$)/.test(path);
}

export function checkUsage(
  files: Map<string, string>,
  contract: unknown,
  allowlist: Allowlist,
): { calls: number; used: string[]; unused: string[] } {
  // Validate injected test inputs as strictly as the on-disk list.
  allowlist = readAllowlist(allowlist);
  const websocketUsage = { used: false };
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
  // Only the entry kinds accepted in the #250 scope decision are traced.
  function capability(node: ts.Node, seen = new Set<ts.Symbol>()): string | undefined {
    if (
      ts.isParenthesizedExpression(node) ||
      ts.isAsExpression(node) ||
      ts.isSatisfiesExpression(node)
    )
      return capability(node.expression, seen);
    const found = symbol(node);
    if (
      ts.isIdentifier(node) &&
      (!found || (node.text === 'globalThis' && !found.valueDeclaration))
    ) {
      if (node.text === 'fetch' || node.text === 'WebSocket') return node.text;
      if (['window', 'globalThis', 'self', 'document'].includes(node.text)) return 'global';
      if (node.text === 'location') return 'location';
    }
    if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) {
      const name = ts.isPropertyAccessExpression(node)
        ? node.name.text
        : ts.isStringLiteralLike(node.argumentExpression)
          ? node.argumentExpression.text
          : undefined;
      const owner = capability(node.expression, seen);
      if (owner === 'global' && name) {
        if (name === 'fetch' || name === 'WebSocket' || name === 'location') return name;
        if (name === 'open') return 'window.open';
      }
      if (owner === 'location' && name && ['assign', 'replace'].includes(name))
        return `location.${name}`;
    }
    const imported = checker.getSymbolAtLocation(node)?.declarations?.find(ts.isImportSpecifier);
    if (imported && (imported.propertyName ?? imported.name).text === 'WebsocketProvider') {
      const declaration = imported.parent.parent.parent;
      if (
        ts.isImportDeclaration(declaration) &&
        ts.isStringLiteral(declaration.moduleSpecifier) &&
        declaration.moduleSpecifier.text === 'y-websocket'
      )
        return 'WebsocketProvider';
    }
    if (!found || seen.has(found)) return undefined;
    seen = new Set(seen).add(found);
    const declaration = found.valueDeclaration;
    if (
      declaration &&
      ts.isBindingElement(declaration) &&
      ts.isObjectBindingPattern(declaration.parent)
    ) {
      const owner = declaration.parent.parent;
      const name = declaration.propertyName ?? declaration.name;
      if (
        ts.isVariableDeclaration(owner) &&
        owner.initializer &&
        ts.isIdentifier(name) &&
        name.text === 'fetch' &&
        capability(owner.initializer, seen) === 'global'
      )
        return 'fetch';
    }
    if (
      declaration &&
      (ts.isParameter(declaration) || ts.isVariableDeclaration(declaration)) &&
      declaration.initializer
    )
      return capability(declaration.initializer, seen);
    return undefined;
  }
  function isFetch(node: ts.Node): boolean {
    return capability(node) === 'fetch';
  }
  function callable(
    node: ts.Node,
    seen = new Set<ts.Node>(),
  ): ts.FunctionDeclaration | ts.FunctionExpression | ts.ArrowFunction | undefined {
    if (seen.has(node)) return undefined;
    seen = new Set(seen).add(node);
    if (
      ts.isParenthesizedExpression(node) ||
      ts.isAsExpression(node) ||
      ts.isSatisfiesExpression(node)
    )
      return callable(node.expression, seen);
    if (ts.isArrowFunction(node) || ts.isFunctionExpression(node)) return node;
    const declaration = symbol(node)?.valueDeclaration;
    if (declaration && ts.isVariableDeclaration(declaration)) {
      return declaration.initializer &&
        ts.isVariableDeclarationList(declaration.parent) &&
        (declaration.parent.flags & ts.NodeFlags.Const) !== 0
        ? callable(declaration.initializer, seen)
        : undefined;
    }
    return declaration && ts.isFunctionDeclaration(declaration) ? declaration : undefined;
  }
  function exitAt(node: ts.Node): Exit | undefined {
    if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
      const args = node.arguments ?? [];
      const name = capability(node.expression);
      if (name === 'fetch') return { kind: 'fetch', url: args[0], options: args[1] };
      if (name === 'WebSocket' || name === 'WebsocketProvider')
        return {
          kind: 'WebSocket',
          url: args[0],
          append: name === 'WebsocketProvider' ? args[1] : undefined,
        };
      if (name === 'window.open' || name?.startsWith('location.'))
        return { kind: 'location', url: args[0] };
      if (ts.isIdentifier(node.expression) && node.expression.text === '__fvoci_link') {
        const line = args[1];
        return {
          kind: 'link',
          url: args[0],
          line: line && ts.isNumericLiteral(line) ? Number(line.text) : undefined,
        };
      }
      if (
        ts.isPropertyAccessExpression(node.expression) &&
        node.expression.name.text === 'setAttribute' &&
        args[0] &&
        ts.isStringLiteralLike(args[0]) &&
        args[0].text === 'href'
      )
        return { kind: 'link', url: args[1] };
    }
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
      if (ts.isPropertyAccessExpression(node.left)) {
        if (node.left.name.text === 'href') return { kind: 'link', url: node.right };
        if (node.left.name.text === 'location' && capability(node.left.expression) === 'global')
          return { kind: 'location', url: node.right };
      }
      if (ts.isIdentifier(node.left) && node.left.text === 'location' && !symbol(node.left))
        return { kind: 'location', url: node.right };
    }
    return undefined;
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
  const callers = new Map<ts.FunctionLikeDeclaration, ts.CallExpression[]>();
  for (const file of sources) {
    walk(file, (node) => {
      if (ts.isCallExpression(node)) {
        const fn = callable(node.expression);
        if (fn) callers.set(fn, [...(callers.get(fn) ?? []), node]);
      }
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
  function parameterArgument(
    parameter: ts.ParameterDeclaration,
    argument: ts.Expression | undefined,
    bindings: Bindings,
  ): ts.Expression | undefined {
    return parameter.initializer && (!argument || isUndefined(argument, bindings))
      ? parameter.initializer
      : argument;
  }
  function* navigationBindings(node: ts.Node, seen = new Set<ts.Node>()): Generator<Bindings> {
    for (let parent = node.parent; !ts.isSourceFile(parent); parent = parent.parent) {
      if (
        !ts.isFunctionDeclaration(parent) &&
        !ts.isFunctionExpression(parent) &&
        !ts.isArrowFunction(parent)
      )
        continue;
      if (seen.has(parent)) {
        yield new Map();
        return;
      }
      seen = new Set(seen).add(parent);
      const sites = callers.get(parent) ?? [];
      if (sites.length === 0) {
        // Runtime callbacks may still capture a statically known outer argument.
        yield* navigationBindings(parent, seen);
        return;
      }
      const written = parameterWrites(parent);
      for (const site of sites) {
        for (const outer of navigationBindings(site, seen)) {
          const bindings = new Map(outer);
          parent.parameters.forEach((parameter, index) => {
            if (!ts.isIdentifier(parameter.name) || written.includes(parameter.name.text)) return;
            const found = symbol(parameter.name);
            const argument = parameterArgument(parameter, site.arguments[index], bindings);
            if (found && argument && !site.arguments.slice(0, index + 1).some(ts.isSpreadElement))
              bindings.set(found, argument);
          });
          yield bindings;
        }
      }
      return;
    }
    yield new Map();
  }
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
      const fn = callable(node.expression);
      if (fn?.body) {
        if (parameterWrites(fn).length > 0) throw new Error('URL function parameter is written');
        const argumentsBySymbol = new Map(bindings);
        fn.parameters.forEach((parameter, index) => {
          const found = symbol(parameter.name);
          const argument = parameterArgument(parameter, node.arguments[index], argumentsBySymbol);
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
  function isUndefined(
    node: ts.Expression,
    bindings: Bindings = new Map(),
    seen = new Set<ts.Node>(),
  ): boolean {
    if (seen.has(node)) return false;
    seen = new Set(seen).add(node);
    if (
      ts.isParenthesizedExpression(node) ||
      ts.isAsExpression(node) ||
      ts.isSatisfiesExpression(node)
    )
      return isUndefined(node.expression, bindings, seen);
    const found = symbol(node);
    const binding = found ? bindings.get(found) : undefined;
    if (binding && !Array.isArray(binding)) return isUndefined(binding, bindings, seen);
    if (ts.isVoidExpression(node)) return true;
    const declaration = found?.valueDeclaration;
    if (
      declaration &&
      ts.isVariableDeclaration(declaration) &&
      declaration.initializer &&
      ts.isVariableDeclarationList(declaration.parent) &&
      (declaration.parent.flags & ts.NodeFlags.Const) !== 0
    )
      return isUndefined(declaration.initializer, bindings, seen);
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
    return files.has(path) ? path : path.replace(/\.(svelte|html)\.ts$/, '.$1');
  }
  function apiHint(node: ts.Node | undefined, seen = new Set<ts.Node>()): boolean {
    if (!node || seen.has(node)) return false;
    seen = new Set(seen).add(node);
    if (ts.isStringLiteralLike(node) && apiPath(node.text)) return true;
    if (ts.isTemplateExpression(node) && apiPath(node.head.text)) return true;
    const declaration = symbol(node)?.valueDeclaration;
    if (
      declaration &&
      ts.isVariableDeclaration(declaration) &&
      apiHint(declaration.initializer, seen)
    )
      return true;
    if (ts.isCallExpression(node)) {
      const fn = callable(node.expression);
      if (fn?.body && apiHint(fn.body, seen)) return true;
    }
    let found = false;
    ts.forEachChild(node, (child) => {
      if (apiHint(child, seen)) found = true;
    });
    return found;
  }

  // Keep origin provenance separate from values(): a literal fvoci.invalid is
  // not the browser host. Unknown path parts are harmless here only after '/'.
  const socketHost = Symbol('browser host');
  const socketScheme = Symbol('browser WebSocket scheme');
  const opaqueSocketPart = Symbol('unknown URL part');
  function browserLocationProperty(node: ts.Node, name: string): boolean {
    if (!ts.isPropertyAccessExpression(node) || node.name.text !== name) return false;
    const location = node.expression;
    if (ts.isIdentifier(location)) return location.text === 'location' && !symbol(location);
    if (!ts.isPropertyAccessExpression(location) || location.name.text !== 'location') return false;
    const owner = location.expression;
    return (
      ts.isIdentifier(owner) &&
      ['window', 'globalThis', 'self', 'document'].includes(owner.text) &&
      (!checker.getSymbolAtLocation(owner) ||
        (owner.text === 'globalThis' && !symbol(owner)?.valueDeclaration))
    );
  }
  function socketParts(node: ts.Expression, seen = new Set<ts.Node>()): (string | symbol)[] {
    if (seen.has(node)) return [opaqueSocketPart];
    seen = new Set(seen).add(node);
    const parts = (expression: ts.Expression): (string | symbol)[] => socketParts(expression, seen);
    if (
      ts.isParenthesizedExpression(node) ||
      ts.isAsExpression(node) ||
      ts.isSatisfiesExpression(node)
    )
      return parts(node.expression);
    if (ts.isStringLiteralLike(node)) return [node.text];
    if (browserLocationProperty(node, 'host')) return [socketHost];
    if (
      ts.isConditionalExpression(node) &&
      ts.isBinaryExpression(node.condition) &&
      node.condition.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken &&
      browserLocationProperty(node.condition.left, 'protocol') &&
      ts.isStringLiteralLike(node.condition.right) &&
      ts.isStringLiteralLike(node.whenTrue) &&
      ts.isStringLiteralLike(node.whenFalse)
    ) {
      const protocol = node.condition.right.text;
      if (
        (protocol === 'https:' && node.whenTrue.text === 'wss' && node.whenFalse.text === 'ws') ||
        (protocol === 'http:' && node.whenTrue.text === 'ws' && node.whenFalse.text === 'wss')
      )
        return [socketScheme];
    }
    if (ts.isTemplateExpression(node))
      return [
        node.head.text,
        ...node.templateSpans.flatMap((span) => [...parts(span.expression), span.literal.text]),
      ];
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.PlusToken)
      return [...parts(node.left), ...parts(node.right)];
    const declaration = symbol(node)?.valueDeclaration;
    if (
      declaration &&
      ts.isVariableDeclaration(declaration) &&
      declaration.initializer &&
      ts.isVariableDeclarationList(declaration.parent) &&
      (declaration.parent.flags & ts.NodeFlags.Const) !== 0
    )
      return parts(declaration.initializer);
    return [opaqueSocketPart];
  }
  function sameSocketOrigin(node: ts.Expression | undefined): boolean {
    if (!node) return false;
    const parts: (string | symbol)[] = [];
    for (const part of socketParts(node)) {
      if (part === '') continue;
      const previous = parts.at(-1);
      if (typeof previous === 'string' && typeof part === 'string')
        parts[parts.length - 1] = previous + part;
      else parts.push(part);
    }
    return (
      parts[0] === socketScheme &&
      parts[1] === '://' &&
      parts[2] === socketHost &&
      (parts.length === 3 || (typeof parts[3] === 'string' && /^[/?#]/.test(parts[3])))
    );
  }
  function inspect(exit: Exit, node: ts.Node, file: ts.SourceFile, strict = false): void {
    const line = exit.line ?? file.getLineAndCharacterOfPosition(node.getStart()).line + 1;
    const location = filename(file) + ':' + String(line);
    try {
      if (exit.kind === 'WebSocket' && !sameSocketOrigin(exit.url))
        throw new Error(
          'unlisted WebSocket origin: browser scheme and host are not statically proven',
        );
      let paths: string[] = [];
      const contexts =
        exit.kind === 'link' || exit.kind === 'location' ? navigationBindings(node) : [new Map()];
      for (const bindings of contexts) {
        try {
          paths.push(...values(exit.url, new Set(), bindings));
        } catch (error: unknown) {
          if (strict || exit.kind === 'fetch' || exit.kind === 'WebSocket' || apiHint(exit.url))
            throw error;
          // Opaque runtime app/content links remain outside the check. Keep
          // checking other callers so an unknown value cannot hide a literal.
        }
      }
      if (exit.append)
        paths = paths.flatMap((path) => values(exit.append).map((room) => path + '/' + room));
      const contractPaths: string[] = [];
      for (const raw of [...new Set(paths)]) {
        const socket = exit.kind === 'WebSocket';
        const path =
          socket || /^(?:https?:)?\/\/fvoci\.invalid(?:\/|$)/.test(raw)
            ? raw.replace(/^(?:(?:https?|wss?):)?\/\/[^/]+/, '')
            : raw;
        canonicalPath(path);
        if (!socket && /^(?:[a-z][\w+.-]*:|\/\/)/i.test(path)) continue;
        const pathname = path.split(/[?#]/, 1)[0] ?? '';
        if (socket) {
          const entry = allowlist.websocket;
          if (!entry || !matches(pathname, entry.path))
            throw new Error('unlisted WebSocket: GET ' + pathname);
          websocketUsage.used = true;
          continue;
        }
        if (exit.kind === 'fetch' && path !== '' && !path.startsWith('/'))
          throw new Error('relative fetch URL is not statically resolvable');
        if (apiPath(pathname)) contractPaths.push(pathname);
      }
      if (contractPaths.length === 0) return;
      const verbs = exit.method
        ? values(exit.method)
        : exit.kind === 'fetch'
          ? fetchMethods(exit.options).map((verb) => verb ?? 'GET')
          : ['GET'];
      for (const path of contractPaths)
        for (const verb of [...new Set(verbs)]) {
          const matched = operations.filter(
            (operation) =>
              operation.startsWith(verb + ' ') && matches(path, operation.slice(verb.length + 1)),
          );
          if (matched.length === 0) throw new Error(verb + ' ' + path + ' not in OpenAPI');
          const operation = matched.sort((a, b) => a.split('{').length - b.split('{').length)[0];
          if (operation) used.add(operation);
        }
      calls++;
    } catch (error: unknown) {
      problems.push(location + ': ' + (error instanceof Error ? error.message : String(error)));
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
          'WebSocket',
          'WebsocketProvider',
          'window.open',
          'location.assign',
          'location.replace',
        ].includes(capability(node) ?? '')
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
        const recipient = ts.isCallExpression(parent) ? callable(parent.expression) : undefined;
        const recipientParameter =
          recipient && ts.isCallExpression(parent)
            ? recipient.parameters[parent.arguments.indexOf(node)]
            : undefined;
        const argument = recipientParameter ? isFetch(recipientParameter.name) : false;
        if (
          !type &&
          !property &&
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
      const exit = exitAt(node);
      if (!ts.isCallExpression(node)) {
        if (exit) inspect(exit, node, file);
        return;
      }
      const called = target(node.expression);
      const callee = callable(node.expression);
      if (callee) {
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
          return;
        }
        if (exit) inspect(exit, node, file);
        else
          inspect(
            { kind: 'request', url: node.arguments[1], method: node.arguments[0] },
            node,
            file,
            true,
          );
      } catch (error: unknown) {
        problems.push(`${location}: ${error instanceof Error ? error.message : String(error)}`);
      }
    });
  }
  for (const [row, count] of forwarded) {
    if (count !== 1) problems.push(`forwarder must delegate exactly once: ${row.file}:${row.name}`);
  }
  if (allowlist.websocket && !websocketUsage.used)
    problems.push(`stale WebSocket item: ${allowlist.websocket.name}`);
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
    const result = checkUsage(readSources(root), contract, readAllowlist(list));
    console.log(
      `${String(result.calls)} web calls, ${String(result.used.length)} used operations, ${String(result.unused.length)} reasoned unused operations`,
    );
  } catch (error: unknown) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
