import type { LocationQueryValue, RouteLocationNormalized, RouteLocationRaw, Router } from 'vue-router';
import type { RouteKey } from '@elegant-router/types';
import { useAuthStore } from '@/store/modules/auth';
import { useRouteStore } from '@/store/modules/route';

/**
 * Create route guard.
 *
 * Cookie session: every protected navigation calls `authStore.ensureAuth()` once,
 * after that the result is cached in the store.
 */
export function createRouteGuard(router: Router) {
  router.beforeEach(async (to, from) => {
    const authStore = useAuthStore();
    const routeStore = useRouteStore();

    // Constant routes must exist before any redirect (login / 403 / 404).
    if (!routeStore.isInitConstantRoute) {
      await routeStore.initConstantRoute();

      // Re-resolve the location against the freshly added routes.
      if (to.name === 'not-found') {
        return redirectToSameLocation(to);
      }
    }

    await authStore.ensureAuth();

    const loginRoute: RouteKey = 'login';
    const rootRoute: RouteKey = 'root';
    const noAuthorizationRoute: RouteKey = '403';

    // Logged-in users should not see the login page.
    if (to.name === loginRoute && authStore.isLogin) {
      return redirectFromLogin(to) || { name: rootRoute, replace: true };
    }

    // Unknown paths are captured by the constant 404 route. When the auth routes
    // are not loaded yet this also covers protected paths such as `/users`:
    // load them once, then re-resolve. Signed-out users must sign in first.
    if (to.name === 'not-found') {
      if (!authStore.isLogin) {
        return {
          name: loginRoute,
          query: { redirect: to.fullPath },
          replace: true
        };
      }

      if (!routeStore.isInitAuthRoute) {
        await routeStore.initAuthRoute();

        return redirectToSameLocation(to);
      }

      return true;
    }

    // Constant routes (403 / 404 / 500 / login) need no session.
    if (to.meta.constant) {
      return true;
    }

    if (!authStore.isLogin) {
      return {
        name: loginRoute,
        query: { redirect: to.fullPath },
        replace: true
      };
    }

    if (!routeStore.isInitAuthRoute) {
      await routeStore.initAuthRoute();

      return redirectToSameLocation(to);
    }

    const routeRoles = to.meta.roles || [];
    const hasRole = routeRoles.some(role => authStore.roles.includes(role));
    const hasAuth = authStore.isStaticSuper || !routeRoles.length || hasRole;

    if (!hasAuth) {
      return { name: noAuthorizationRoute };
    }

    // Keep `from` referenced for future guards / debugging parity with upstream.
    void from;

    return true;
  });
}

function redirectToSameLocation(to: RouteLocationNormalized): RouteLocationRaw {
  return {
    path: to.path,
    replace: true,
    query: to.query,
    hash: to.hash
  };
}

/** Only allow in-app absolute paths as post-login redirects. */
export function getSafeRedirect(redirect: LocationQueryValue | LocationQueryValue[] | undefined | null) {
  const value = Array.isArray(redirect) ? redirect[0] : redirect;

  if (typeof value !== 'string') return null;
  if (!value.startsWith('/') || value.startsWith('//')) return null;

  return value;
}

function redirectFromLogin(to: RouteLocationNormalized) {
  const redirect = getSafeRedirect(to.query.redirect);

  if (!redirect) return null;

  return { path: redirect, replace: true } as RouteLocationRaw;
}
