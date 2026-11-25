import type { AxiosError } from 'axios';
import { $t } from '@/locales';
import type { RequestInstanceState } from './type';

export interface ApiErrorInfo {
  /** HTTP status code, 0 means network/unknown error */
  status: number;
  /** business error code from the backend, may be empty */
  code: string;
  /** human readable message from the backend, may be empty */
  message: string;
  /** extra details from the backend, used by forms if present */
  details: unknown;
}

/**
 * Extract a normalized error from an axios error.
 *
 * Supports both `{ error: { code, message } }` and `{ code, message }` error bodies.
 */
export function getApiError(error: unknown): ApiErrorInfo {
  const axiosError = error as AxiosError<Record<string, any>> | undefined;
  const response = axiosError?.response;
  const body = response?.data || {};
  const nested = body?.error && typeof body.error === 'object' ? body.error : undefined;

  const rawCode = nested?.code ?? body?.code ?? '';
  const rawMessage = nested?.message ?? body?.message ?? body?.msg ?? '';
  const details = nested?.details ?? body?.details ?? null;

  let status = response?.status || 0;

  // Some backends return the HTTP-like status in the body only.
  if (!status) {
    const bodyStatus = Number(body?.status);
    if (Number.isFinite(bodyStatus) && bodyStatus >= 100) {
      status = bodyStatus;
    }
  }

  const message = typeof rawMessage === 'string' ? rawMessage : '';

  return {
    status,
    code: String(rawCode || ''),
    message,
    details
  };
}

/** Whether the error is a 404 (backend endpoint not implemented). */
export function isNotFoundError(error: unknown) {
  return getApiError(error).status === 404;
}

export function showErrorMsg(state: RequestInstanceState, message: string) {
  if (!state.errMsgStack?.length) {
    state.errMsgStack = [];
  }

  const isExist = state.errMsgStack.includes(message);

  if (!isExist) {
    state.errMsgStack.push(message);

    window.$message?.error(message, {
      onLeave: () => {
        state.errMsgStack = state.errMsgStack.filter(msg => msg !== message);

        setTimeout(() => {
          state.errMsgStack = [];
        }, 5000);
      }
    });
  }
}

/** Show a generic (non-sensitive) error message. */
export function showServiceUnavailable(state: RequestInstanceState) {
  showErrorMsg(state, $t('request.serviceUnavailable'));
}
