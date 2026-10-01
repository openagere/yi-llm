import fs from 'node:fs';
import path from 'node:path';
import postcss from 'postcss';

const root = process.cwd();
const styleFiles = fs.readdirSync(path.join(root, 'src/styles')).filter((name) => name.endsWith('.css')).map((name) => `src/styles/${name}`);
const roots = new Map();
const parsed = new Map();
for (const file of styleFiles) {
  const full = path.join(root, file);
  const ast = postcss.parse(fs.readFileSync(full, 'utf8'), { from: full });
  parsed.set(file, ast);
  ast.walkDecls(/^--[\w-]+$/, (decl) => roots.set(decl.prop.slice(2), file));
}

const errors = [];
for (const [file, ast] of parsed) {
  ast.walkDecls((decl) => {
    if (/^\.(?:color|border-color)$/.test(decl.prop)) errors.push(`${file}: malformed declaration "${decl.prop}"`);
    if (decl.prop.startsWith('--')) {
      const selfReference = new RegExp(`var\\(\\s*${decl.prop.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?:[,)]|\\s)`).test(decl.value);
      if (selfReference) errors.push(`${file}: self-referential token ${decl.prop}`);
    }
    const refs = [...decl.value.matchAll(/var\(\s*(--[\w-]+)/g)].map((match) => match[1].slice(2));
    for (const name of refs) {
      if (name.startsWith('radix-')) continue;
      if (!roots.has(name)) errors.push(`${file}: undefined var(--${name}) in ${decl.prop}`);
    }
  });
}

const nonTokenStyles = styleFiles.filter((file) => file !== 'src/styles/tokens.css');
for (const file of nonTokenStyles) {
  const content = fs.readFileSync(path.join(root, file), 'utf8');
  if (/#(?:[\da-f]{3}|[\da-f]{6}|[\da-f]{8})\b/i.test(content)) errors.push(`${file}: hard-coded hex color outside tokens.css`);
  if (/var\(--(?:accent-[\d]|ink-|muted-\d|line-\d|surface-\d|amber-\d|blue-\d|danger-\d)/.test(content)) errors.push(`${file}: legacy per-color token reference`);
}

if (errors.length) {
  console.error(errors.join('\n'));
  process.exitCode = 1;
} else {
  console.log(`Theme styles valid: ${styleFiles.length} CSS files, ${roots.size} declared custom properties, no unresolved vars or hard-coded style colors.`);
}
