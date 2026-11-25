<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref } from 'vue';
import { $t } from '@/locales';
import { useAuthStore } from '@/store/modules/auth';
import { useFormRules, useNaiveForm } from '@/hooks/common/form';
import { useRouterPush } from '@/hooks/common/router';

defineOptions({
  name: 'PwdLogin'
});

const authStore = useAuthStore();
const { redirectFromLogin } = useRouterPush();
const { formRef, validate } = useNaiveForm();

interface FormModel {
  email: string;
  password: string;
}

const model: FormModel = reactive({
  email: '',
  password: ''
});

const errorMessage = ref('');
const lockedSeconds = ref(0);

let lockTimer: ReturnType<typeof setInterval> | null = null;

const isLocked = computed(() => lockedSeconds.value > 0);

const rules = computed<Record<keyof FormModel, App.Global.FormRule[]>>(() => {
  const { formRules } = useFormRules();

  return {
    email: formRules.email,
    password: [{ required: true, message: $t('form.password.required') }]
  };
});

function startLock(seconds: number) {
  lockedSeconds.value = seconds;

  if (lockTimer) {
    clearInterval(lockTimer);
  }

  lockTimer = setInterval(() => {
    lockedSeconds.value = Math.max(0, lockedSeconds.value - 1);

    if (lockedSeconds.value === 0 && lockTimer) {
      clearInterval(lockTimer);
      lockTimer = null;
    }
  }, 1000);
}

onBeforeUnmount(() => {
  if (lockTimer) {
    clearInterval(lockTimer);
    lockTimer = null;
  }
});

async function handleSubmit() {
  if (authStore.loginLoading || isLocked.value) return;

  errorMessage.value = '';

  try {
    await validate();
  } catch {
    errorMessage.value = $t('page.login.error.invalidInput');
    return;
  }

  const error = await authStore.login(model.email.trim(), model.password);

  if (!error) {
    await redirectFromLogin();
    return;
  }

  if (error.status === 401) {
    errorMessage.value = $t('page.login.error.invalidCredentials');
    return;
  }

  if (error.status === 429) {
    const retryAfter = Number((error.details as Record<string, unknown> | null)?.retry_after) || 30;
    startLock(retryAfter);
    errorMessage.value = $t('page.login.error.rateLimited', { seconds: retryAfter });
    return;
  }

  if (error.status >= 500 || error.status === 0) {
    errorMessage.value = $t('page.login.error.serviceUnavailable');
    return;
  }

  errorMessage.value = error.message || $t('page.login.error.invalidInput');
}
</script>

<template>
  <NForm ref="formRef" :model="model" :rules="rules" size="large" :show-label="false" @keyup.enter="handleSubmit">
    <NFormItem path="email">
      <NInput
        v-model:value="model.email"
        autocomplete="email"
        inputmode="email"
        :placeholder="$t('page.login.emailPlaceholder')"
        :disabled="authStore.loginLoading"
      />
    </NFormItem>
    <NFormItem path="password">
      <NInput
        v-model:value="model.password"
        type="password"
        show-password-on="click"
        autocomplete="current-password"
        :placeholder="$t('page.login.passwordPlaceholder')"
        :disabled="authStore.loginLoading"
      />
    </NFormItem>
    <NSpace vertical :size="18">
      <NAlert v-if="errorMessage" type="error" :bordered="false" role="alert">
        {{ errorMessage }}
      </NAlert>
      <NButton
        type="primary"
        size="large"
        round
        block
        :loading="authStore.loginLoading"
        :disabled="authStore.loginLoading || isLocked"
        @click="handleSubmit"
      >
        {{ isLocked ? $t('page.login.locked', { seconds: lockedSeconds }) : $t('page.login.submit') }}
      </NButton>
    </NSpace>
  </NForm>
</template>

<style scoped></style>
