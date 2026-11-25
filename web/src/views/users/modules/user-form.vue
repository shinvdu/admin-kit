<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import type { FormInst, FormRules, SelectOption } from 'naive-ui';
import { fetchCreateUser } from '@/service/api';
import { getApiError } from '@/service/request';
import { $t } from '@/locales';
import { useFormRules } from '@/hooks/common/form';

defineOptions({
  name: 'UserForm'
});

const show = defineModel<boolean>('show', { required: true });

const emit = defineEmits<{
  created: [];
}>();

const formRef = ref<FormInst | null>(null);
const submitting = ref(false);
const formError = ref('');
const emailError = ref('');

interface FormModel {
  email: string;
  password: string;
  confirmPassword: string;
  role: 'admin' | 'user';
}

const model = reactive<FormModel>({
  email: '',
  password: '',
  confirmPassword: '',
  role: 'user'
});

const roleOptions: SelectOption[] = [
  { label: $t('common.role.admin'), value: 'admin' },
  { label: $t('common.role.user'), value: 'user' }
];

const { formRules, createConfirmPwdRule } = useFormRules();

const rules = computed<FormRules>(() => ({
  email: formRules.email,
  password: [
    { required: true, message: $t('form.password.required') },
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
  confirmPassword: createConfirmPwdRule(model.password),
  role: [{ required: true, message: $t('form.role.required') }]
}));

function resetModel() {
  model.email = '';
  model.password = '';
  model.confirmPassword = '';
  model.role = 'user';
  formError.value = '';
  emailError.value = '';
  formRef.value?.restoreValidation();
}

watch(show, value => {
  if (value) {
    resetModel();
  }
});

async function handleSubmit() {
  formError.value = '';
  emailError.value = '';

  try {
    await formRef.value?.validate();
  } catch {
    formError.value = $t('page.users.validationError');
    return;
  }

  submitting.value = true;

  const { error } = await fetchCreateUser({
    email: model.email.trim(),
    password: model.password,
    role: model.role
  });

  submitting.value = false;

  if (!error) {
    show.value = false;
    emit('created');
    return;
  }

  const apiError = getApiError(error);

  if (apiError.status === 409 || apiError.code === 'EMAIL_TAKEN') {
    emailError.value = $t('page.users.emailTaken');
    return;
  }

  if (apiError.status === 400) {
    formError.value = apiError.message || $t('page.users.validationError');
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
    :title="$t('page.users.createTitle')"
    class="w-480px max-w-90vw"
    :mask-closable="!submitting"
    :closable="!submitting"
  >
    <NForm ref="formRef" :model="model" :rules="rules" label-placement="top">
      <NFormItem path="email" :validation-status="emailError ? 'error' : undefined" :feedback="emailError || undefined">
        <template #label>{{ $t('page.users.createEmail') }}</template>
        <NInput
          v-model:value="model.email"
          autocomplete="email"
          :placeholder="$t('page.users.createEmail')"
          @input="emailError = ''"
        />
      </NFormItem>
      <NFormItem path="password">
        <template #label>{{ $t('page.users.createPassword') }}</template>
        <NInput
          v-model:value="model.password"
          type="password"
          show-password-on="click"
          autocomplete="new-password"
          :placeholder="$t('page.users.createPassword')"
        />
      </NFormItem>
      <NFormItem path="confirmPassword">
        <template #label>{{ $t('page.users.createConfirmPassword') }}</template>
        <NInput
          v-model:value="model.confirmPassword"
          type="password"
          show-password-on="click"
          autocomplete="new-password"
          :placeholder="$t('page.users.createConfirmPassword')"
        />
      </NFormItem>
      <NFormItem path="role">
        <template #label>{{ $t('page.users.createRole') }}</template>
        <NSelect v-model:value="model.role" :options="roleOptions" />
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
