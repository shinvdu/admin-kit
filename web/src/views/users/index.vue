<script setup lang="ts">
import { computed, h, onMounted, reactive, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useDebounceFn } from '@vueuse/core';
import { NButton, NPopconfirm, NSelect, NSpace, NTag } from 'naive-ui';
import type { DataTableColumns, SelectOption } from 'naive-ui';
import { fetchUserList, fetchUpdateUser } from '@/service/api';
import { getApiError } from '@/service/request';
import { useAuthStore } from '@/store/modules/auth';
import { $t } from '@/locales';
import { formatDateTime } from '@/utils/format';
import UserForm from './modules/user-form.vue';
import ResetPassword from './modules/reset-password.vue';

defineOptions({
  name: 'UsersPage'
});

const authStore = useAuthStore();
const route = useRoute();
const router = useRouter();

const loading = ref(false);
const items = ref<Api.Admin.User[]>([]);
const nextCursor = ref<string | null>(null);
const cursorStack = ref<(string | null)[]>([null]);
const pageIndex = ref(0);
const pageSize = ref(20);
const loadError = ref('');

const filters = reactive<{
  q: string;
  role: Api.Admin.Role | '';
  status: Api.Admin.UserStatus | '';
}>({
  q: '',
  role: '',
  status: ''
});

const createVisible = ref(false);
const resetVisible = ref(false);
const resetTarget = ref<Api.Admin.User | null>(null);

const currentUserId = computed(() => authStore.user?.id || '');

const roleOptions: SelectOption[] = [
  { label: $t('common.role.owner'), value: 'owner' },
  { label: $t('common.role.admin'), value: 'admin' },
  { label: $t('common.role.user'), value: 'user' }
];

const filterRoleOptions: SelectOption[] = [
  { label: $t('page.users.allRoles'), value: '' },
  ...roleOptions
];

const statusOptions: SelectOption[] = [
  { label: $t('page.users.allStatus'), value: '' },
  { label: $t('page.users.statusActive'), value: 'active' },
  { label: $t('page.users.statusDisabled'), value: 'disabled' }
];

const assignableRoleOptions: SelectOption[] = [
  { label: $t('common.role.admin'), value: 'admin' },
  { label: $t('common.role.user'), value: 'user' }
];

const pageSizeOptions = [
  { label: '10', value: 10 },
  { label: '20', value: 20 },
  { label: '50', value: 50 }
];

function userRowKey(row: Api.Admin.User) {
  return row.id;
}

function actionError(error: unknown) {
  const apiError = getApiError(error);

  if (apiError.code === 'SELF_ACTION') {
    window.$message?.warning($t('page.users.selfAction'));
    return;
  }

  if (apiError.code === 'LAST_ADMIN') {
    window.$message?.warning($t('page.users.lastAdmin'));
    return;
  }

  if (apiError.status === 403) {
    window.$message?.error($t('route.403'));
    return;
  }

  if (apiError.status >= 500 || apiError.status === 0) {
    // The request layer already shows a generic message.
    return;
  }

  window.$message?.error(apiError.message || $t('page.users.validationError'));
}

async function changeRole(row: Api.Admin.User, role: 'admin' | 'user') {
  if (row.role === role) return;

  const { data, error } = await fetchUpdateUser(row.id, { role });

  if (error) {
    actionError(error);
    return;
  }

  if (data) {
    Object.assign(row, data);
  }

  window.$message?.success($t('page.users.roleChanged'));

  if (row.id === currentUserId.value) {
    await authStore.fetchMe();
  }
}

async function toggleDisabled(row: Api.Admin.User) {
  const { data, error } = await fetchUpdateUser(row.id, { disabled: !row.disabled });

  if (error) {
    actionError(error);
    return;
  }

  if (data) {
    Object.assign(row, data);
  }

  window.$message?.success($t('page.users.statusChanged'));
}

function openResetPassword(row: Api.Admin.User) {
  resetTarget.value = row;
  resetVisible.value = true;
}

const columns = computed<DataTableColumns<Api.Admin.User>>(() => [
  {
    key: 'email',
    title: $t('page.users.email'),
    minWidth: 240,
    ellipsis: { tooltip: true }
  },
  {
    key: 'role',
    title: $t('page.users.role'),
    width: 170,
    render: row => {
      if (row.role === 'owner') {
        return h(NTag, { type: 'warning', bordered: false }, { default: () => $t('common.role.owner') });
      }

      return h(NSelect, {
        value: row.role,
        options: assignableRoleOptions,
        size: 'small',
        disabled: row.id === currentUserId.value,
        onUpdateValue: (value: 'admin' | 'user') => changeRole(row, value)
      });
    }
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
    width: 170,
    render: row => formatDateTime(row.created_at)
  },
  {
    key: 'actions',
    title: $t('page.users.actions'),
    width: 240,
    fixed: 'right',
    render: row => {
      const isSelf = row.id === currentUserId.value;

      return h(NSpace, { size: 8, wrap: true }, {
        default: () => [
          h(
            NPopconfirm,
            {
              onPositiveClick: () => toggleDisabled(row)
            },
            {
              trigger: () =>
                h(
                  NButton,
                  {
                    size: 'small',
                    type: row.disabled ? 'success' : 'error',
                    tertiary: true,
                    disabled: isSelf
                  },
                  { default: () => (row.disabled ? $t('page.users.enable') : $t('page.users.disable')) }
                ),
              default: () =>
                row.disabled ? $t('page.users.enableConfirmContent') : $t('page.users.disableConfirmContent')
            }
          ),
          h(
            NButton,
            { size: 'small', tertiary: true, onClick: () => openResetPassword(row) },
            { default: () => $t('page.users.resetPassword') }
          )
        ]
      });
    }
  }
]);

async function load() {
  loading.value = true;
  loadError.value = '';

  const cursor = cursorStack.value[pageIndex.value] ?? null;

  const { data, error } = await fetchUserList({
    limit: pageSize.value,
    cursor,
    q: filters.q,
    role: filters.role,
    status: filters.status
  });

  loading.value = false;

  if (error) {
    const apiError = getApiError(error);

    if (apiError.status !== 401) {
      loadError.value = $t('page.users.loadFailed');
    }

    items.value = [];
    nextCursor.value = null;
    return;
  }

  items.value = data.items;
  nextCursor.value = data.next_cursor;
}

function resetPaging() {
  cursorStack.value = [null];
  pageIndex.value = 0;
}

function reloadFromFirstPage() {
  resetPaging();
  load();
}

const debouncedSearch = useDebounceFn(reloadFromFirstPage, 300);

watch(() => filters.q, debouncedSearch);
watch(() => filters.role, reloadFromFirstPage);
watch(() => filters.status, reloadFromFirstPage);
watch(pageSize, reloadFromFirstPage);

function resetFilters() {
  filters.q = '';
  filters.role = '';
  filters.status = '';
  reloadFromFirstPage();
}

function goNext() {
  if (!nextCursor.value) return;

  cursorStack.value = [...cursorStack.value, nextCursor.value];
  pageIndex.value += 1;
  load();
}

function goPrevious() {
  if (pageIndex.value <= 0) return;

  pageIndex.value -= 1;
  load();
}

function handleCreated() {
  window.$message?.success($t('page.users.createSuccess'));
  load();
}

function handlePasswordReset() {
  window.$message?.success($t('page.users.resetPasswordSuccess'));
}

onMounted(() => {
  load();

  if (route.query.create === '1') {
    createVisible.value = true;
    router.replace({ name: 'users' });
  }
});

const canGoPrevious = computed(() => pageIndex.value > 0);
const loadedText = computed(() => $t('page.users.loaded', { count: items.value.length }));
</script>

<template>
  <NSpace vertical :size="16">
    <div class="flex flex-wrap items-center justify-between gap-12px">
      <div>
        <h2 class="text-22px font-500">{{ $t('page.users.title') }}</h2>
        <p class="mt-4px text-14px op-70">{{ $t('page.users.description') }}</p>
      </div>
      <NButton type="primary" @click="createVisible = true">{{ $t('page.users.createUser') }}</NButton>
    </div>

    <NCard :bordered="false" class="card-wrapper">
      <div class="flex flex-wrap items-center gap-12px">
        <NInput
          v-model:value="filters.q"
          clearable
          class="w-240px"
          :placeholder="$t('page.users.searchPlaceholder')"
        />
        <NSelect v-model:value="filters.role" class="w-160px" :options="filterRoleOptions" />
        <NSelect v-model:value="filters.status" class="w-160px" :options="statusOptions" />
        <NButton @click="resetFilters">{{ $t('page.users.resetFilters') }}</NButton>
      </div>

      <NAlert v-if="loadError" class="mt-16px" type="error" :bordered="false">
        {{ loadError }}
      </NAlert>

      <NDataTable
        class="mt-16px"
        :columns="columns"
        :data="items"
        :loading="loading"
        :row-key="userRowKey"
        :scroll-x="980"
        remote
      />

      <NEmpty v-if="!loading && !items.length" class="py-24px" :description="$t('page.users.empty')" />

      <div class="mt-16px flex flex-wrap items-center justify-between gap-12px">
        <div class="flex items-center gap-8px text-14px">
          <span class="op-70">{{ loadedText }}</span>
          <span class="op-70">{{ $t('page.users.pageSize') }}</span>
          <NSelect v-model:value="pageSize" class="w-80px" size="small" :options="pageSizeOptions" />
        </div>
        <NSpace>
          <NButton :disabled="!canGoPrevious" @click="goPrevious">{{ $t('page.users.previous') }}</NButton>
          <NButton :disabled="!nextCursor" @click="goNext">{{ $t('page.users.next') }}</NButton>
        </NSpace>
      </div>
    </NCard>

    <UserForm v-model:show="createVisible" @created="handleCreated" />
    <ResetPassword v-model:show="resetVisible" :user="resetTarget" @success="handlePasswordReset" />
  </NSpace>
</template>

<style scoped></style>
