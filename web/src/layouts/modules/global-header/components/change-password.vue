<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import type { FormInst, FormRules } from 'naive-ui';
import { fetchChangePassword } from '@/service/api';
import { getApiError } from '@/service/request';
import { $t } from '@/locales';
import { useFormRules } from '@/hooks/common/form';

defineOptions({
  name: 'ChangePassword'
});

const show = defineModel<boolean>('show', { required: true });

const emit = defineEmits<{
  success: [];
}>();

const formRef = ref<FormInst | null>(null);
const submitting = ref(false);
const formError = ref('');
const currentPasswordError = ref('');

interface FormModel {
  currentPassword: string;
  newPassword: string;
  confirmPassword: string;
}

const model = reactive<FormModel>({
  currentPassword: '',
  newPassword: '',
  confirmPassword: ''
});

const { createConfirmPwdRule } = useFormRules();

const rules = computed<FormRules>(() => ({
  currentPassword: [{ required: true, message: $t('form.password.required') }],
  newPassword: [
    { required: true, message: $t('form.newPassword.required') },
    {
      validator: (_rule, value: string) => {
        if (value.length < 8 || value.length > 128) {
          return new Error($t('page.users.passwordLength'));
        }
        return true;
      },
      trigger: ['input', 'blur']
    }
  ],
  confirmPassword: createConfirmPwdRule(model.newPassword)
}));

watch(show, value => {
  if (value) {
    model.currentPassword = '';
    model.newPassword = '';
    model.confirmPassword = '';
    formError.value = '';
    currentPasswordError.value = '';
    formRef.value?.restoreValidation();
  }
});

async function handleSubmit() {
  formError.value = '';
  currentPasswordError.value = '';

  try {
    await formRef.value?.validate();
  } catch {
    formError.value = $t('page.users.validationError');
    return;
  }

  submitting.value = true;

  const { error } = await fetchChangePassword({
    current_password: model.currentPassword,
    new_password: model.newPassword
  });

  submitting.value = false;

  if (!error) {
    show.value = false;
    emit('success');
    return;
  }

  const apiError = getApiError(error);

  if (apiError.status === 401) {
    currentPasswordError.value = $t('page.users.currentPasswordInvalid');
    return;
  }

  if (apiError.status === 403) {
    formError.value = $t('route.403');
    return;
  }

  if (apiError.status >= 500 || apiError.status === 0) {
    formError.value = $t('request.serviceUnavailable');
    return;
  }

  formError.value = apiError.message || $t('page.users.validationError');
}
</script>

<template>
  <NModal
    v-model:show="show"
    preset="card"
    :title="$t('page.users.changePasswordTitle')"
    class="w-480px max-w-90vw"
    :mask-closable="!submitting"
    :closable="!submitting"
  >
    <NForm ref="formRef" :model="model" :rules="rules" label-placement="top">
      <NFormItem
        path="currentPassword"
        :validation-status="currentPasswordError ? 'error' : undefined"
        :feedback="currentPasswordError || undefined"
      >
        <template #label>{{ $t('page.users.currentPassword') }}</template>
        <NInput
          v-model:value="model.currentPassword"
          type="password"
          show-password-on="click"
          autocomplete="current-password"
          @input="currentPasswordError = ''"
        />
      </NFormItem>
      <NFormItem path="newPassword">
        <template #label>{{ $t('page.users.newPassword') }}</template>
        <NInput
          v-model:value="model.newPassword"
          type="password"
          show-password-on="click"
          autocomplete="new-password"
        />
      </NFormItem>
      <NFormItem path="confirmPassword">
        <template #label>{{ $t('page.users.confirmPassword') }}</template>
        <NInput
          v-model:value="model.confirmPassword"
          type="password"
          show-password-on="click"
          autocomplete="new-password"
        />
      </NFormItem>
      <NAlert v-if="formError" type="error" :bordered="false">{{ formError }}</NAlert>
    </NForm>
    <template #footer>
      <NSpace justify="end">
        <NButton :disabled="submitting" @click="show = false">{{ $t('page.users.cancel') }}</NButton>
        <NButton type="primary" :loading="submitting" @click="handleSubmit">{{ $t('page.users.save') }}</NButton>
      </NSpace>
    </template>
  </NModal>
</template>

<style scoped></style>
