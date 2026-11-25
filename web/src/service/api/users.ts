import { request } from '../request';

type RawUser = Partial<Api.Admin.User> & {
  created_at?: string | number | null;
  updated_at?: string | number | null;
};

/** Convert epoch milliseconds or an ISO string to an ISO 8601 UTC string. */
function toISOString(value: string | number | null | undefined): string {
  if (value === null || value === undefined || value === '') return '';

  if (typeof value === 'number') {
    return new Date(value).toISOString();
  }

  if (/^\d+$/.test(value)) {
    const timestamp = Number(value);
    // Accept both seconds and milliseconds.
    return new Date(value.length <= 10 ? timestamp * 1000 : timestamp).toISOString();
  }

  return value;
}

function normalizeUser(raw: RawUser): Api.Admin.User {
  return {
    id: String(raw.id || ''),
    email: String(raw.email || ''),
    role: (raw.role || 'user') as Api.Admin.Role,
    disabled: Boolean(raw.disabled),
    created_at: toISOString(raw.created_at),
    updated_at: toISOString(raw.updated_at)
  };
}

function normalizeList(raw: any): Api.Admin.UserListResponse {
  const items = Array.isArray(raw?.items) ? raw.items : [];
  const nextCursor = raw?.next_cursor ?? raw?.nextCursor ?? null;

  return {
    items: items.map((item: RawUser) => normalizeUser(item)),
    next_cursor: nextCursor || null,
    total: typeof raw?.total === 'number' ? raw.total : null
  };
}

/** Cursor-paginated user list. `cursor` is opaque and must be passed back unchanged. */
export async function fetchUserList(params: Api.Admin.UserListParams = {}) {
  const query: Record<string, string | number> = {
    limit: params.limit ?? 20
  };

  if (params.cursor) query.cursor = params.cursor;
  if (params.q?.trim()) query.q = params.q.trim();
  if (params.role) query.role = params.role;
  if (params.status) query.status = params.status;

  const result = await request<Api.Admin.UserListResponse>({
    url: '/admin/users',
    params: query
  });

  if (result.data) {
    result.data = normalizeList(result.data);
  }

  return result;
}

/** Create a user (role must be admin or user). */
export async function fetchCreateUser(data: Api.Admin.CreateUserParams) {
  const result = await request<Api.Admin.User>({
    url: '/admin/users',
    method: 'post',
    data
  });

  if (result.data) {
    result.data = normalizeUser(result.data as RawUser);
  }

  return result;
}

/** Update role / disabled state / password of a user. */
export async function fetchUpdateUser(id: string, data: Api.Admin.UpdateUserParams) {
  const result = await request<Api.Admin.User>({
    url: `/admin/users/${encodeURIComponent(id)}`,
    method: 'patch',
    data
  });

  if (result.data) {
    result.data = normalizeUser(result.data as RawUser);
  }

  return result;
}

/** Dashboard statistics (optional endpoint). */
export async function fetchAdminStats() {
  return request<Api.Admin.Stats>({ url: '/admin/stats' });
}
