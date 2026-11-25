<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import type { FormInst, FormRules } from 'naive-ui';
import { fetchUpdateUser } from '@/service/api';
import { getApiError } from '@/service/request';
import { $t } from '@/locales';
import { useFormRules } from '@/hooks/common/form';

defineOptions({
  name: 'ResetPassword'
});

const show = defineModel<boolean>('show', { required: true });

const props = defineProps<{
  user: Api.Admin.User | null;
}>();

const emit = defineEmits<{
  success: [];
}>();

const formRef = ref<FormInst | null>(null);
const submitting = ref(false);
const formError = ref('');

interface FormModel {
  password: string;
  confirmPassword: string;
}

const model = reactive<FormModel>({
  password: '',
  confirmPassword: ''
});

const { createConfirmPwdRule } = useFormRules();

const rules = computed<FormRules>(() => ({
  password: [
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
  confirmPassword: createConfirmPwdRule(model.password)
}));

watch(show, value => {
  if (value) {
    model.password = '';
    model.confirmPassword = '';
    formError.value = '';
    formRef.value?.restoreValidation();
  }
});

async function handleSubmit() {
  if (!props.user) return;

  formError.value = '';

  try {
    await formRef.value?.validate();
  } catch {
    formError.value = $t('page.users.validationError');
    return;
  }

  submitting.value = true;

  const { error } = await fetchUpdateUser(props.user.id, { password: model.password });

  submitting.value = false;

  if (!error) {
    show.value = false;
    emit('success');
    return;
  }

  const apiError = getApiError(error);

  if (apiError.code === 'SELF_ACTION') {
    formError.value = $t('page.users.selfAction');
    return;
  }

  if (apiError.code === 'LAST_ADMIN') {
    formError.value = $t('page.users.lastAdmin');
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
    :title="$t('page.users.resetPasswordTitle')"
    class="w-480px max-w-90vw"
    :mask-closable="!submitting"
    :closable="!submitting"
  >
    <NForm ref="formRef" :model="model" :rules="rules" label-placement="top">
      <NFormItem path="password">
        <template #label>{{ $t('page.users.newPassword') }}</template>
        <NInput
          v-model:value="model.password"
          type="password"
          show-password-on="click"
          autocomplete="new-password"
          :placeholder="$t('page.users.newPassword')"
        />
      </NFormItem>
      <NFormItem path="confirmPassword">
        <template #label>{{ $t('page.users.confirmPassword') }}</template>
        <NInput
          v-model:value="model.confirmPassword"
          type="password"
          show-password-on="click"
          autocomplete="new-password"
          :placeholder="$t('page.users.confirmPassword')"
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
