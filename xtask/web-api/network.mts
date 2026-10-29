import ts from 'typescript';

export interface NetworkPolicy {
  websockets: { path: string; reason: string }[];
  sites: { file: string; kind: string; expression: string; reason: string }[];
}
export interface NetworkEntry {
  file: string;
  line: number;
  kind: string;
  expression: string;
  classification: '대조함' | '이유 있는 예외';
  reason: string;
  operations: string[];
}
export interface Exit {
  kind: string;
  url?: ts.Expression | undefined;
  options?: ts.Expression | undefined;
  method?: ts.Expression | undefined;
  verb?: string | undefined;
  line?: number | undefined;
  append?: ts.Expression | undefined;
}

function object(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value))
    throw new Error('network policy must contain objects');
  return Object.fromEntries(Object.entries(value));
}
function text(value: unknown): string {
  if (typeof value !== 'string' || value.trim() === '')
    throw new Error('network exception needs nonempty text');
  return value;
}
export function readNetworkPolicy(value: unknown): NetworkPolicy {
  const input = object(value);
  if (!Array.isArray(input.websockets) || !Array.isArray(input.sites))
    throw new Error('network policy needs websockets and sites arrays');
  return {
    websockets: input.websockets.map((value: unknown) => {
      const row = object(value);
      return { path: text(row.path), reason: text(row.reason) };
    }),
    sites: input.sites.map((value: unknown) => {
      const row = object(value);
      return {
        file: text(row.file),
        kind: text(row.kind),
        expression: text(row.expression),
        reason: text(row.reason),
      };
    }),
  };
}
export function canonicalPath(url: string): void {
  const path = url.split(/[?#]/, 1)[0] ?? '';
  // Browsers treat encoded dots and backslashes as path syntax before sending.
  const dots = path.replace(/%2e/gi, '.');
  if (/(?:^|\/)\.{1,2}(?:\/|$)/.test(dots) || /\\|%2f|%5c/i.test(path)) {
    throw new Error(`noncanonical URL or dot segment: ${url}`);
  }
}
export function compact(expression: string): string {
  return expression.replace(/\s+/g, ' ').trim();
}

const nativeNames = new Set([
  'fetch',
  'Request',
  'XMLHttpRequest',
  'EventSource',
  'WebSocket',
  'Worker',
  'SharedWorker',
]);
const hosts = new Set(['window', 'globalThis', 'self', 'document']);
type SymbolAt = (node: ts.Node) => ts.Symbol | undefined;

// Resolve capabilities independently of their local spelling, including default
// parameters. A caller then rejects escapes and writes instead of ignoring them.
export function capability(
  node: ts.Node,
  checker: ts.TypeChecker,
  symbol: SymbolAt,
  seen = new Set<ts.Symbol>(),
): string | undefined {
  if (
    ts.isParenthesizedExpression(node) ||
    ts.isAsExpression(node) ||
    ts.isSatisfiesExpression(node)
  )
    return capability(node.expression, checker, symbol, seen);
  if (
    ts.isIdentifier(node) &&
    (!symbol(node) || (node.text === 'globalThis' && !symbol(node)?.valueDeclaration))
  ) {
    if (nativeNames.has(node.text)) return node.text;
    if (hosts.has(node.text)) return 'global';
    if (node.text === 'location' || node.text === 'navigator') return node.text;
  }
  if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) {
    const name = ts.isPropertyAccessExpression(node)
      ? node.name.text
      : ts.isStringLiteralLike(node.argumentExpression)
        ? node.argumentExpression.text
        : undefined;
    const owner = capability(node.expression, checker, symbol, seen);
    if (owner === 'global' && name) {
      if (nativeNames.has(name)) return name;
      if (name === 'location' || name === 'navigator') return name;
      if (name === 'open') return 'window.open';
    }
    if (owner === 'location' && name && ['assign', 'replace', 'reload'].includes(name))
      return `location.${name}`;
    if (owner === 'navigator' && name === 'sendBeacon') return 'sendBeacon';
    if (owner === 'xhr' && name === 'open') return 'XMLHttpRequest.open';
  }
  if (
    ts.isNewExpression(node) &&
    capability(node.expression, checker, symbol, seen) === 'XMLHttpRequest'
  )
    return 'xhr';
  const raw = checker.getSymbolAtLocation(node);
  const imported = raw?.declarations?.find(ts.isImportSpecifier);
  if (imported && (imported.propertyName ?? imported.name).text === 'WebsocketProvider') {
    const declaration = imported.parent.parent.parent;
    if (
      ts.isImportDeclaration(declaration) &&
      ts.isStringLiteral(declaration.moduleSpecifier) &&
      declaration.moduleSpecifier.text === 'y-websocket'
    )
      return 'WebsocketProvider';
  }
  const found = symbol(node);
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
      capability(owner.initializer, checker, symbol, seen) === 'global'
    )
      return 'fetch';
  }
  if (declaration && ts.isParameter(declaration) && declaration.initializer)
    return capability(declaration.initializer, checker, symbol, seen);
  if (
    declaration &&
    ts.isVariableDeclaration(declaration) &&
    declaration.initializer &&
    ts.isVariableDeclarationList(declaration.parent)
  )
    return capability(declaration.initializer, checker, symbol, seen);
  return undefined;
}

export function networkExit(
  node: ts.Node,
  checker: ts.TypeChecker,
  symbol: SymbolAt,
): Exit | undefined {
  if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
    const args = node.arguments ?? [];
    const name = capability(node.expression, checker, symbol);
    if (name === 'fetch' || name === 'Request')
      return { kind: name, url: args[0], options: args[1] };
    if (name === 'XMLHttpRequest.open')
      return { kind: 'XMLHttpRequest', url: args[1], method: args[0] };
    if (name === 'XMLHttpRequest') return { kind: 'XMLHttpRequest constructor' };
    if (name === 'EventSource' || name === 'WebSocket')
      return { kind: name, url: args[0], verb: 'GET' };
    if (name === 'WebsocketProvider')
      return { kind: 'WebSocket', url: args[0], append: args[1], verb: 'GET' };
    if (name === 'sendBeacon') return { kind: name, url: args[0], verb: 'POST' };
    if (name === 'window.open' || name?.startsWith('location.'))
      return { kind: name, url: args[0], verb: 'GET' };
    if (name === 'Worker' || name === 'SharedWorker')
      return { kind: name, url: args[0], verb: 'GET' };
    if (
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.name.text === 'default' &&
      args[0] &&
      ts.isObjectLiteralExpression(args[0])
    ) {
      const path = args[0].properties.find(
        (property) =>
          ts.isPropertyAssignment(property) && property.name.getText() === 'module_or_path',
      );
      if (path && ts.isPropertyAssignment(path))
        return { kind: 'WASM loader', url: path.initializer, verb: 'GET' };
    }
    if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword)
      return { kind: 'import', url: args[0], verb: 'GET' };
    if (ts.isIdentifier(node.expression) && node.expression.text === '__fvoci_network') {
      const kind = args[0];
      const line = args[2];
      if (kind && ts.isStringLiteral(kind))
        return {
          kind: kind.text,
          url: args[1],
          line: line && ts.isNumericLiteral(line) ? Number(line.text) : undefined,
          method: args[3],
          verb: args[3] ? undefined : 'GET',
        };
    }
    if (
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.name.text === 'setAttribute'
    ) {
      const name = args[0];
      if (
        name &&
        ts.isStringLiteralLike(name) &&
        ['href', 'src', 'action', 'srcdoc'].includes(name.text)
      )
        return { kind: name.text, url: args[1], verb: 'GET' };
      if (name && !ts.isStringLiteralLike(name)) return { kind: 'dynamic attribute', url: args[1] };
    }
  }
  if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
    const lhs = node.left;
    if (ts.isPropertyAccessExpression(lhs)) {
      if (['href', 'src', 'action', 'srcdoc', 'innerHTML', 'outerHTML'].includes(lhs.name.text))
        return { kind: lhs.name.text, url: node.right, verb: 'GET' };
      if (lhs.name.text === 'location' && capability(lhs.expression, checker, symbol) === 'global')
        return { kind: 'location.assign', url: node.right, verb: 'GET' };
    }
    if (ts.isIdentifier(lhs) && lhs.text === 'location' && !symbol(lhs))
      return { kind: 'location.assign', url: node.right, verb: 'GET' };
  }
  return undefined;
}
