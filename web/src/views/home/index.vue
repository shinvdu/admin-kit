<script setup lang="ts">
import { computed, h, onMounted, ref } from 'vue';
import { NButton, NTag } from 'naive-ui';
import type { DataTableColumns } from 'naive-ui';
import { fetchAdminStats, fetchUserList } from '@/service/api';
import { getApiError } from '@/service/request';
import { $t } from '@/locales';
import { useRouterPush } from '@/hooks/common/router';
import { useAppStore } from '@/store/modules/app';
import { formatDateTime } from '@/utils/format';

defineOptions({
  name: 'HomePage'
});

const appStore = useAppStore();
const { routerPushByKey } = useRouterPush();

const gap = computed(() => (appStore.isMobile ? 0 : 16));

const stats = ref<Api.Admin.Stats | null>(null);
const statsVisible = ref(false);
const recentUsers = ref<Api.Admin.User[]>([]);
const loading = ref(true);

const columns = computed<DataTableColumns<Api.Admin.User>>(() => [
  {
    key: 'email',
    title: $t('page.users.email'),
    minWidth: 220,
    ellipsis: { tooltip: true }
  },
  {
    key: 'role',
    title: $t('page.users.role'),
    width: 120,
    render: row =>
      h(
        NTag,
        { type: row.role === 'owner' ? 'warning' : row.role === 'admin' ? 'info' : 'default', bordered: false },
        { default: () => $t(`common.role.${row.role}`) }
      )
  },
  {
    key: 'status',
    title: $t('page.users.status'),
    width: 110,
    render: row =>
      h(
        NTag,
        { type: row.disabled ? 'error' : 'success', bordered: false },
        { default: () => (row.disabled ? $t('page.users.statusDisabled') : $t('page.users.statusActive')) }
      )
  },
  {
    key: 'created_at',
    title: $t('page.users.createdAt'),
    width: 160,
    render: row => formatDateTime(row.created_at)
  }
]);

function userRowKey(row: Api.Admin.User) {
  return row.id;
}

function openCreateUser() {
  routerPushByKey('users', { query: { create: '1' } });
}

function openUsers() {
  routerPushByKey('users');
}

async function loadDashboard() {
  loading.value = true;

  const [statsResult, usersResult] = await Promise.all([
    fetchAdminStats(),
    fetchUserList({ limit: 5 })
  ]);

  if (!statsResult.error && statsResult.data) {
    stats.value = statsResult.data;
    statsVisible.value = true;
  } else {
    // The stats endpoint is optional; hide cards on 404 or any failure.
    stats.value = null;
    statsVisible.value = false;

    if (statsResult.error && getApiError(statsResult.error).status === 401) {
      return;
    }
  }

  if (!usersResult.error && usersResult.data) {
    recentUsers.value = usersResult.data.items;
  } else {
    recentUsers.value = [];
  }

  loading.value = false;
}

onMounted(loadDashboard);
</script>

<template>
  <NSpace vertical :size="16">
    <div class="flex flex-wrap items-center justify-between gap-12px">
      <div>
        <h2 class="text-22px font-500">{{ $t('page.home.title') }}</h2>
        <p class="mt-4px text-14px op-70">{{ $t('page.home.subtitle') }}</p>
      </div>
      <NSpace>
        <NButton type="primary" @click="openCreateUser">{{ $t('page.home.createUser') }}</NButton>
        <NButton @click="openUsers">{{ $t('page.home.viewAll') }}</NButton>
      </NSpace>
    </div>

    <NGrid v-if="statsVisible && stats" :x-gap="gap" :y-gap="16" cols="1 s:3" responsive="screen">
      <NGi>
        <NCard :bordered="false" class="card-wrapper">
          <NStatistic :label="$t('page.home.stats.totalUsers')" :value="stats.total_users" />
        </NCard>
      </NGi>
      <NGi>
        <NCard :bordered="false" class="card-wrapper">
          <NStatistic :label="$t('page.home.stats.activeUsers')" :value="stats.active_users" />
        </NCard>
      </NGi>
      <NGi>
        <NCard :bordered="false" class="card-wrapper">
          <NStatistic :label="$t('page.home.stats.disabledUsers')" :value="stats.disabled_users" />
        </NCard>
      </NGi>
    </NGrid>

    <NCard :bordered="false" class="card-wrapper" :title="$t('page.home.recentUsers')">
      <template #header-extra>
        <NButton text type="primary" @click="openUsers">{{ $t('page.home.viewAll') }}</NButton>
      </template>
      <NDataTable
        :columns="columns"
        :data="recentUsers"
        :loading="loading"
        :row-key="userRowKey"
        :scroll-x="760"
        size="small"
      />
      <NEmpty v-if="!loading && !recentUsers.length" class="py-24px" :description="$t('page.home.empty')" />
    </NCard>
  </NSpace>
</template>

<style scoped></style>
