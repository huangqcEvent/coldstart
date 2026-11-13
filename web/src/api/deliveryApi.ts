/**
 * 多仓库交付总览 API。
 *
 * 只读取 Rust 端已聚合的本地快照，不在浏览器侧拼接或推断 PR、Checks 与 Review 状态。
 */
import { invoke } from '@tauri-apps/api/core';

import type { DeliveryOverviewView } from './types';

export async function readDeliveryOverview(path: string) {
  return invoke<DeliveryOverviewView>('load_delivery_overview', { path });
}
