import { request } from '../request';

/** Login with email and password; the backend sets an HttpOnly cookie. */
export function fetchLogin(email: string, password: string) {
  return request<Api.Auth.LoginResponse>({
    url: '/auth/login',
    method: 'post',
    data: { email, password } satisfies Api.Auth.LoginParams
  });
}

/** Get the current session user. */
export function fetchGetUserInfo() {
  return request<Api.Auth.MeResponse>({ url: '/auth/me' });
}

/** Clear the backend cookie session. */
export function fetchLogout() {
  return request<void>({ url: '/auth/logout', method: 'post' });
}

/** Change the current user's password (optional backend feature). */
export function fetchChangePassword(data: Api.Auth.ChangePasswordParams) {
  return request<void>({ url: '/auth/password', method: 'post', data });
}
