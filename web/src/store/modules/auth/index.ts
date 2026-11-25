import { computed, reactive, ref } from 'vue';
import { defineStore } from 'pinia';
import { useLoading } from '@sa/hooks';
import { fetchGetUserInfo, fetchLogin, fetchLogout } from '@/service/api';
import { getApiError } from '@/service/request';
import type { ApiErrorInfo } from '@/service/request';
import { router } from '@/router';
import { SetupStoreId } from '@/enum';
import { useRouteStore } from '../route';
import { useTabStore } from '../tab';

const EMPTY_USER_INFO: Api.Auth.UserInfo = {
  userId: '',
  userName: '',
  roles: [],
  buttons: []
};

export const useAuthStore = defineStore(SetupStoreId.Auth, () => {
  const { loading: loginLoading, startLoading, endLoading } = useLoading();

  /** Session user, the only source of truth is the HttpOnly cookie + GET /auth/me. */
  const user = ref<Api.Auth.User | null>(null);

  /** Whether the initial /auth/me check has completed. */
  const initialized = ref(false);

  /** Shape expected by Soybean Admin (user menu, route guard). */
  const userInfo = reactive<Api.Auth.UserInfo>({ ...EMPTY_USER_INFO });

  let initPromise: Promise<boolean> | null = null;
  let handlingUnauthorized = false;
  let checkingAuth = false;

  const roles = computed<string[]>(() => (user.value ? [user.value.role] : []));

  /** is super role in static route */
  const isStaticSuper = computed(() => {
    const { VITE_AUTH_ROUTE_MODE, VITE_STATIC_SUPER_ROLE } = import.meta.env;

    return VITE_AUTH_ROUTE_MODE === 'static' && roles.value.includes(VITE_STATIC_SUPER_ROLE);
  });

  const isLogin = computed(() => Boolean(user.value));

  const isAdmin = computed(() => Boolean(user.value && ['owner', 'admin'].includes(user.value.role)));

  function applyUser(nextUser: Api.Auth.User | null) {
    user.value = nextUser;
    Object.assign(userInfo, nextUser
      ? {
          userId: nextUser.id,
          userName: nextUser.email,
          roles: [nextUser.role],
          buttons: []
        }
      : { ...EMPTY_USER_INFO });
  }

  function clearFrontendState() {
    applyUser(null);
    initialized.value = true;

    useTabStore().resetTabs();
  }

  /** Check the cookie session once; subsequent navigations reuse the cached state. */
  async function ensureAuth() {
    if (initialized.value) {
      return isLogin.value;
    }

    if (!initPromise) {
      initPromise = (async () => {
        checkingAuth = true;

        const { data, error } = await fetchGetUserInfo();

        if (!error && data?.user) {
          applyUser(data.user);
        } else {
          applyUser(null);
        }

        initialized.value = true;
        checkingAuth = false;

        return isLogin.value;
      })().finally(() => {
        checkingAuth = false;
        initPromise = null;
      });
    }

    return initPromise;
  }

  /** Force re-fetch /auth/me (e.g. after changing the current user's role). */
  async function fetchMe() {
    const { data, error } = await fetchGetUserInfo();

    if (!error && data?.user) {
      applyUser(data.user);
      initialized.value = true;
      return true;
    }

    return false;
  }

  /**
   * Login with email + password.
   *
   * @returns API error when login fails, `null` on success
   */
  async function login(email: string, password: string): Promise<ApiErrorInfo | null> {
    startLoading();

    try {
      const { data, error } = await fetchLogin(email, password);

      if (!error && data?.user) {
        applyUser(data.user);
        initialized.value = true;
        return null;
      }

      return getApiError(error);
    } finally {
      endLoading();
    }
  }

  /** Clear the backend session and all frontend state, then go to the login page. */
  async function logout() {
    try {
      await fetchLogout();
    } finally {
      clearFrontendState();
      useRouteStore().resetStore();
      await router.replace({ name: 'login' });
    }
  }

  /** Called by the request layer on 401. Single-flight: only one redirect. */
  async function handleUnauthorized() {
    if (handlingUnauthorized) return;

    handlingUnauthorized = true;
    clearFrontendState();

    // During the initial /auth/me check the route guard owns the redirect.
    if (checkingAuth) {
      handlingUnauthorized = false;
      return;
    }

    try {
      const current = router.currentRoute.value;

      if (current.name !== 'login') {
        const redirect = current.fullPath && current.fullPath !== '/' ? current.fullPath : undefined;

        await router.replace({
          name: 'login',
          query: redirect ? { redirect } : {}
        });
      }
    } finally {
      setTimeout(() => {
        handlingUnauthorized = false;
      }, 1000);
    }
  }

  /** Frontend-only cleanup, compatible with upstream route store calls. */
  function resetStore() {
    clearFrontendState();
  }

  return {
    user,
    userInfo,
    initialized,
    roles,
    isStaticSuper,
    isLogin,
    isAdmin,
    loginLoading,
    ensureAuth,
    fetchMe,
    login,
    logout,
    handleUnauthorized,
    resetStore
  };
});
