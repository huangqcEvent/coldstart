import { invoke } from '@tauri-apps/api/core';

import type { DeliveryOverviewView } from './types';

export async function readDeliveryOverview(path: string) {
  return invoke<DeliveryOverviewView>('load_delivery_overview', { path });
}
