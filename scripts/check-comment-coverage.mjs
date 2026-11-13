/**
 * 统计第一方生产代码的注释覆盖率。
 *
 * 统计口径：
 * - 模块覆盖率：文件开头是否有解释职责与边界的模块文档；
 * - 复杂行为覆盖率：有实质分支或多个职责的公开函数、公开组件是否有紧邻的中文文档注释；
 * - 类型覆盖率：公开类型是否有说明，作为辅助指标输出。
 *
 * 测试代码、测试夹具、生成物和第三方依赖不进入统计。门禁只要求模块与
 * 公开行为达到 70%，避免为了追求物理行比例而堆砌无信息量的行尾注释。
 */
import { readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = process.cwd();
const minimumCoverage = 70;
const complexFunctionLineThreshold = 12;
const complexFunctionDecisionThreshold = 2;

const areas = [
  { name: 'core', roots: ['crates/core/src'] },
  { name: 'storage', roots: ['crates/storage/src'] },
  { name: 'adapters', roots: ['crates/adapters/src'] },
  { name: 'app', roots: ['crates/app/src'] },
  { name: 'mcp', roots: ['crates/mcp/src'] },
  { name: 'cli', roots: ['crates/cli/src'] },
  { name: 'desktop', roots: ['apps/desktop/src-tauri/src'] },
  { name: 'web', roots: ['web/src'] },
];

const sourceExtensions = new Set(['.rs', '.ts', '.tsx']);

/** 递归收集需要统计的源码文件。 */
function collectFiles(directory) {
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'tests' || entry.name === 'node_modules' || entry.name === 'target') {
        continue;
      }
      files.push(...collectFiles(path));
      continue;
    }
    if (!sourceExtensions.has(entry.name.slice(entry.name.lastIndexOf('.')))) {
      continue;
    }
    if (
      entry.name.endsWith('.test.tsx') ||
      entry.name.endsWith('.test.ts') ||
      entry.name === 'setup.ts'
    ) {
      continue;
    }
    files.push(path);
  }
  return files;
}

/** 判断文件开头是否存在模块级说明。 */
function hasModuleDocumentation(lines, extension) {
  const first = lines.find((line) => line.trim().length > 0)?.trim() ?? '';
  return extension === '.rs' ? first.startsWith('//!') : first.startsWith('/**');
}

/** 判断指定行之前是否有连续 Rust 文档注释，允许中间出现属性宏。 */
function hasRustDocumentation(lines, index) {
  let cursor = index - 1;
  while (cursor >= 0) {
    const text = lines[cursor].trim();
    if (text.startsWith('#[') && text.endsWith(']')) {
      cursor -= 1;
      continue;
    }
    if (text.endsWith(']')) {
      let attributeStart = cursor;
      while (attributeStart >= 0 && !lines[attributeStart].trim().startsWith('#[')) {
        attributeStart -= 1;
      }
      if (attributeStart >= 0) {
        cursor = attributeStart - 1;
        continue;
      }
    }
    break;
  }
  return cursor >= 0 && lines[cursor].trim().startsWith('///');
}

/** 判断指定行之前是否有 TypeScript/JSDoc 或连续行注释。 */
function hasTypeScriptDocumentation(lines, index) {
  let cursor = index - 1;
  while (cursor >= 0 && lines[cursor].trim().startsWith('@')) {
    cursor -= 1;
  }
  const current = lines[cursor]?.trim() ?? '';
  if (current.endsWith('*/')) {
    while (cursor >= 0) {
      if (lines[cursor].trim().startsWith('/**')) {
        return true;
      }
      cursor -= 1;
    }
    return false;
  }
  return current.startsWith('//');
}

/** 粗略删除字符串和行注释，避免括号与关键字干扰结构统计。 */
function sanitizeSourceLine(line) {
  return line
    .replace(/\/\/.*$/u, '')
    .replace(/"(?:\\.|[^"\\])*"/gu, '""')
    .replace(/'(?:\\.|[^'\\])*'/gu, "''")
    .replace(/`(?:\\.|[^`\\])*`/gu, '``');
}

/** 估算函数体行数和分支点数量，判断它是否需要行为说明。 */
function measureFunction(lines, index) {
  let depth = 0;
  let opened = false;
  let end = index;
  for (let cursor = index; cursor < lines.length; cursor += 1) {
    const cleaned = sanitizeSourceLine(lines[cursor]);
    for (const character of cleaned) {
      if (character === '{') {
        opened = true;
        depth += 1;
      } else if (character === '}' && opened) {
        depth -= 1;
        if (depth === 0) {
          end = cursor;
          break;
        }
      }
    }
    if (opened && depth === 0) {
      end = cursor;
      break;
    }
  }
  if (!opened) {
    return { bodyLines: 0, decisionPoints: 0 };
  }
  const body = lines.slice(index, end + 1);
  const bodyLines = body.filter((line) => line.trim().length > 0).length;
  const decisionPoints = body.reduce(
    (count, line) =>
      count +
      (sanitizeSourceLine(line).match(/\b(?:if|match|for|while|loop)\b|&&|\|\|/gu)?.length ?? 0),
    0,
  );
  return { bodyLines, decisionPoints };
}

/** 解析单个源码文件中的公开行为和公开类型。 */
function inspectFile(path) {
  const extension = path.slice(path.lastIndexOf('.'));
  const lines = readFileSync(path, 'utf8').split(/\r?\n/);
  const functions = [];
  const types = [];

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (extension === '.rs') {
      const functionMatch = line.match(
        /^\s*pub\s+(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)/,
      );
      if (functionMatch) {
        const metrics = measureFunction(lines, index);
        functions.push({
          name: functionMatch[1],
          documented: hasRustDocumentation(lines, index),
          requiresDocumentation:
            metrics.bodyLines >= complexFunctionLineThreshold &&
            metrics.decisionPoints >= complexFunctionDecisionThreshold,
        });
      }
      const typeMatch = line.match(
        /^\s*pub\s+(?:struct|enum|trait|type|const|static)\s+(\w+)/,
      );
      if (typeMatch) {
        types.push({
          name: typeMatch[1],
          documented: hasRustDocumentation(lines, index),
        });
      }
      continue;
    }

    const functionMatch = line.match(
      /^\s*export\s+(?:default\s+)?(?:async\s+)?function\s+(\w+)/,
    );
    if (functionMatch) {
      const metrics = measureFunction(lines, index);
      functions.push({
        name: functionMatch[1],
        documented: hasTypeScriptDocumentation(lines, index),
        requiresDocumentation:
          metrics.bodyLines >= complexFunctionLineThreshold &&
          metrics.decisionPoints >= complexFunctionDecisionThreshold,
      });
    }
    const arrowMatch = line.match(
      /^\s*export\s+const\s+(\w+)\s*=\s*(?:async\s*)?(?:\([^)]*\)|\w+)\s*=>/,
    );
    if (arrowMatch) {
      const metrics = measureFunction(lines, index);
      functions.push({
        name: arrowMatch[1],
        documented: hasTypeScriptDocumentation(lines, index),
        requiresDocumentation:
          metrics.bodyLines >= complexFunctionLineThreshold &&
          metrics.decisionPoints >= complexFunctionDecisionThreshold,
      });
    }
    const typeMatch = line.match(/^\s*export\s+(?:interface|type|class)\s+(\w+)/);
    if (typeMatch) {
      types.push({
        name: typeMatch[1],
        documented: hasTypeScriptDocumentation(lines, index),
      });
    }
  }

  return {
    moduleDocumented: hasModuleDocumentation(lines, extension),
    functions,
    types,
  };
}

/** 计算一个区域内需要展示和门禁的统计值。 */
function summarize(files) {
  const summary = {
    files: 0,
    modules: 0,
    functions: 0,
    documentedFunctions: 0,
    complexFunctions: 0,
    documentedComplexFunctions: 0,
    types: 0,
    documentedTypes: 0,
    undocumentedFunctions: [],
  };
  for (const file of files) {
    const result = inspectFile(file.path);
    summary.files += 1;
    summary.modules += result.moduleDocumented ? 1 : 0;
    summary.functions += result.functions.length;
    summary.types += result.types.length;
    for (const item of result.functions) {
      summary.documentedFunctions += item.documented ? 1 : 0;
      if (item.requiresDocumentation) {
        summary.complexFunctions += 1;
        summary.documentedComplexFunctions += item.documented ? 1 : 0;
      }
      if (item.requiresDocumentation && !item.documented) {
        summary.undocumentedFunctions.push(`${file.path}: ${item.name}`);
      }
    }
    for (const item of result.types) {
      summary.documentedTypes += item.documented ? 1 : 0;
    }
  }
  return summary;
}

/** 将比例格式化为稳定的一位小数。 */
function percentage(numerator, denominator) {
  return denominator === 0 ? 100 : (numerator / denominator) * 100;
}

const rows = areas.map((area) => {
  const files = area.roots.flatMap((directory) => collectFiles(join(root, directory)));
  return {
    name: area.name,
    files: files.map((path) => ({ path: relative(root, path), absolute: path })),
  };
});

const totals = summarize(rows.flatMap((row) => row.files));
console.log('区域      文件  模块覆盖   复杂行为覆盖       全部公开行为   公开类型');
for (const row of rows) {
  const summary = summarize(row.files);
  console.log(
    `${row.name.padEnd(9)} ${String(summary.files).padStart(4)}  ` +
      `${summary.modules}/${summary.files}`.padEnd(10) +
      ` ${summary.documentedComplexFunctions}/${summary.complexFunctions}`.padEnd(18) +
      ` ${summary.documentedFunctions}/${summary.functions}`.padEnd(13) +
      ` ${summary.documentedTypes}/${summary.types}`,
  );
}

const moduleCoverage = percentage(totals.modules, totals.files);
const functionCoverage = percentage(
  totals.documentedComplexFunctions,
  totals.complexFunctions,
);
const typeCoverage = percentage(totals.documentedTypes, totals.types);
console.log('');
console.log(`模块注释覆盖率：${moduleCoverage.toFixed(1)}%`);
console.log(`复杂公开行为注释覆盖率：${functionCoverage.toFixed(1)}%`);
console.log(
  `全部公开行为注释覆盖率：${percentage(
    totals.documentedFunctions,
    totals.functions,
  ).toFixed(1)}%（辅助指标）`,
);
console.log(`公开类型注释覆盖率：${typeCoverage.toFixed(1)}%（辅助指标）`);

if (moduleCoverage < minimumCoverage || functionCoverage < minimumCoverage) {
  console.error(
    `\n注释门禁未通过：模块和复杂公开行为覆盖率都必须达到 ${minimumCoverage}%。`,
  );
  if (totals.undocumentedFunctions.length > 0) {
    console.error('\n未覆盖的公开行为：');
    for (const item of totals.undocumentedFunctions.slice(0, 80)) {
      console.error(`- ${item}`);
    }
    if (totals.undocumentedFunctions.length > 80) {
      console.error(`- 其余 ${totals.undocumentedFunctions.length - 80} 项省略`);
    }
  }
  process.exitCode = 1;
}
