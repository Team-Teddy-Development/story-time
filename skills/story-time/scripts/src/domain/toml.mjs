// The TOML subset the design models use: comments, bare or quoted keys, basic and literal
// strings, integers, booleans, arrays (multi-line), inline tables, [table] and [[array]] headers.
// Anything else (dates, floats, dotted keys, multi-line strings) is rejected with its line number,
// so a model never parses to something its author did not write. Pure, no I/O.

const BARE_KEY = /[A-Za-z0-9_-]/;

export function parseToml(text) {
  const src = text.replace(/\r\n/g, '\n');
  let pos = 0;
  let line = 1;
  const root = {};
  const headers = new Set();
  let current = root;

  const fail = (message, at = line) => {
    throw new Error(`line ${at}: ${message}`);
  };
  const peek = () => src[pos];
  const advance = () => {
    if (src[pos] === '\n') line++;
    pos++;
  };
  const skipSpaces = () => {
    while (peek() === ' ' || peek() === '\t') advance();
  };
  const skipComment = () => {
    if (peek() === '#') while (pos < src.length && peek() !== '\n') advance();
  };
  // Inside arrays and inline tables: whitespace, newlines and comments.
  const skipBlank = () => {
    for (;;) {
      skipSpaces();
      skipComment();
      if (peek() !== '\n') return;
      advance();
    }
  };
  const endOfLine = () => {
    skipSpaces();
    skipComment();
    if (pos < src.length && peek() !== '\n') fail('expected end of line');
  };

  const key = () => {
    let k;
    if (peek() === '"' || peek() === "'") k = string();
    else {
      const start = pos;
      while (pos < src.length && BARE_KEY.test(peek())) advance();
      k = src.slice(start, pos);
      if (!k) fail(`expected a key, got "${peek() ?? 'end of file'}"`);
    }
    skipSpaces();
    if (peek() === '.') fail('dotted keys are not supported');
    return k;
  };

  const assign = (table, k, v) => {
    if (Object.hasOwn(table, k)) fail(`duplicate key "${k}"`);
    table[k] = v;
  };

  const string = () => {
    const quote = peek();
    if (src.startsWith(quote.repeat(3), pos)) fail('multi-line strings are not supported');
    const start = pos;
    advance();
    while (peek() !== quote) {
      if (pos >= src.length || peek() === '\n') fail('unterminated string');
      if (quote === '"' && peek() === '\\') advance();
      advance();
    }
    advance();
    const raw = src.slice(start, pos);
    if (quote === "'") return raw.slice(1, -1);
    try {
      return JSON.parse(raw);
    } catch {
      return fail(`unsupported escape in ${raw}`);
    }
  };

  const array = () => {
    const out = [];
    const opened = line;
    advance();
    for (;;) {
      skipBlank();
      if (pos >= src.length) fail('unterminated array', opened);
      if (peek() === ']') break;
      out.push(value());
      skipBlank();
      if (pos >= src.length) fail('unterminated array', opened);
      if (peek() === ',') advance();
      else if (peek() !== ']') fail('expected "," or "]" in array');
    }
    advance();
    return out;
  };

  const inlineTable = () => {
    const out = {};
    const opened = line;
    advance();
    for (;;) {
      skipBlank();
      if (pos >= src.length) fail('unterminated inline table', opened);
      if (peek() === '}') break;
      const k = key();
      if (peek() !== '=') fail(`expected "=" after "${k}"`);
      advance();
      skipSpaces();
      assign(out, k, value());
      skipBlank();
      if (peek() === ',') advance();
      else if (peek() !== '}') fail('expected "," or "}" in inline table');
    }
    advance();
    return out;
  };

  const value = () => {
    const c = peek();
    if (c === '"' || c === "'") return string();
    if (c === '[') return array();
    if (c === '{') return inlineTable();
    const word = /^(true|false|[+-]?\d[\d_]*)(?=[\s,\]}#]|$)/.exec(src.slice(pos));
    if (!word) fail('unsupported value');
    for (let i = 0; i < word[1].length; i++) advance();
    if (word[1] === 'true' || word[1] === 'false') return word[1] === 'true';
    return Number(word[1].replace(/_/g, ''));
  };

  const header = () => {
    const isArray = src.startsWith('[[', pos);
    advance();
    if (isArray) advance();
    skipSpaces();
    const name = key();
    for (const close of isArray ? ']]' : ']') {
      if (peek() !== close) fail(`expected "${isArray ? ']]' : ']'}" after table name`);
      advance();
    }
    if (isArray) {
      if (Object.hasOwn(root, name) && !(Array.isArray(root[name]) && headers.has(`[[${name}]]`))) {
        fail(`"${name}" is not an array of tables`);
      }
      headers.add(`[[${name}]]`);
      root[name] ??= [];
      current = {};
      root[name].push(current);
    } else {
      if (headers.has(name) || Object.hasOwn(root, name)) fail(`table "${name}" is defined twice`);
      headers.add(name);
      current = root[name] = {};
    }
    endOfLine();
  };

  while (pos < src.length) {
    skipSpaces();
    skipComment();
    if (pos >= src.length) break;
    if (peek() === '\n') {
      advance();
      continue;
    }
    if (peek() === '[') {
      header();
      continue;
    }
    const k = key();
    if (peek() !== '=') fail(`expected "=" after "${k}"`);
    advance();
    skipSpaces();
    assign(current, k, value());
    endOfLine();
  }
  return root;
}
