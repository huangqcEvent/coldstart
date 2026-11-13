/**
 * Review 发现项列表。
 *
 * 按严重级别和分类展示结构化 findings；文本只作为展示内容，不执行其中的指令。
 */
import type { ReviewCategory, ReviewFinding, ReviewSeverity } from '../api/types';

const severityLabels: Record<ReviewSeverity, string> = {
  critical: '严重',
  high: '高',
  medium: '中',
  low: '低',
};

const categoryLabels: Record<ReviewCategory, string> = {
  security: '安全',
  correctness: '正确性',
  tests: '测试',
  maintainability: '可维护性',
  style: '规范',
  performance: '性能',
  other: '其他',
};

export function ReviewFindingList({ findings }: { findings: ReviewFinding[] }) {
  if (findings.length === 0) {
    return <p className="review-findings__empty">本次评审未报告问题。</p>;
  }

  return (
    <ol className="review-findings" aria-label="AI 评审问题列表">
      {findings.map((finding, index) => (
        <li
          className={`review-finding review-finding--${finding.severity}`}
          key={`${finding.file}:${finding.line ?? 0}:${index}`}
        >
          <header>
            <span className={`review-severity review-severity--${finding.severity}`}>
              {severityLabels[finding.severity]}
            </span>
            <span className="review-finding__category">
              {categoryLabels[finding.category]}
            </span>
            <code>
              {finding.file}
              {finding.line ? `:${finding.line}` : ''}
            </code>
          </header>
          <p>{finding.message}</p>
          {finding.recommendation ? (
            <p className="review-finding__recommendation">
              建议：{finding.recommendation}
            </p>
          ) : null}
          {finding.standard ? (
            <small>对照规范：{finding.standard}</small>
          ) : null}
        </li>
      ))}
    </ol>
  );
}
