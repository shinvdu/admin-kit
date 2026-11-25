<script setup lang="ts">
import { computed, ref } from 'vue';
import type { VNode } from 'vue';
import { NTag } from 'naive-ui';
import { $t } from '@/locales';
import { useAuthStore } from '@/store/modules/auth';
import { useSvgIcon } from '@/hooks/common/icon';
import { useRouterPush } from '@/hooks/common/router';
import ChangePassword from './change-password.vue';

defineOptions({
  name: 'UserAvatar'
});

const authStore = useAuthStore();
const { toLogin } = useRouterPush();
const { SvgIconVNode } = useSvgIcon();

const changePasswordVisible = ref(false);

function loginOrRegister() {
  toLogin();
}

type DropdownKey = 'changePassword' | 'logout';

type DropdownOption =
  | {
      key: DropdownKey;
      label: string;
      icon?: () => VNode;
    }
  | {
      type: 'divider';
      key: string;
    };

const options = computed(() => {
  const opts: DropdownOption[] = [
    {
      label: $t('page.users.changePassword'),
      key: 'changePassword',
      icon: SvgIconVNode({ icon: 'ph:key', fontSize: 18 })
    },
    {
      type: 'divider',
      key: 'divider'
    },
    {
      label: $t('common.logout'),
      key: 'logout',
      icon: SvgIconVNode({ icon: 'ph:sign-out', fontSize: 18 })
    }
  ];

  return opts;
});

function logout() {
  window.$dialog?.info({
    title: $t('common.tip'),
    content: $t('common.logoutConfirm'),
    positiveText: $t('common.confirm'),
    negativeText: $t('common.cancel'),
    onPositiveClick: () => {
      authStore.logout();
    }
  });
}

async function handlePasswordChanged() {
  window.$message?.success($t('page.users.changePasswordSuccess'));
  await authStore.logout();
}

function handleDropdown(key: DropdownKey) {
  if (key === 'logout') {
    logout();
  } else if (key === 'changePassword') {
    changePasswordVisible.value = true;
  } else {
    // no other options
  }
}
</script>

<template>
  <NButton v-if="!authStore.isLogin" quaternary @click="loginOrRegister">
    {{ $t('page.login.title') }}
  </NButton>
  <NDropdown v-else placement="bottom" trigger="click" :options="options" @select="handleDropdown">
    <div>
      <ButtonIcon>
        <SvgIcon icon="ph:user-circle" class="text-icon-large" />
        <span class="text-16px font-medium">{{ authStore.userInfo.userName }}</span>
        <NTag v-if="authStore.user" size="small" :bordered="false" type="info">
          {{ $t(`common.role.${authStore.user.role}`) }}
        </NTag>
      </ButtonIcon>
    </div>
  </NDropdown>
  <ChangePassword v-model:show="changePasswordVisible" @success="handlePasswordChanged" />
</template>

<style scoped></style>
