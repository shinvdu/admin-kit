declare namespace Api {
  /**
   * namespace Admin
   *
   * admin api module: "admin/users", "admin/stats"
   */
  namespace Admin {
    type Role = Api.Auth.Role;

    type UserStatus = 'active' | 'disabled';

    interface User {
      id: string;
      email: string;
      role: Role;
      disabled: boolean;
      /** ISO 8601 UTC */
      created_at: string;
      /** ISO 8601 UTC */
      updated_at: string;
    }

    interface UserListParams {
      limit?: number;
      cursor?: string | null;
      q?: string;
      role?: Role | '';
      status?: UserStatus | '';
    }

    interface UserListResponse {
      items: User[];
      next_cursor: string | null;
      total?: number | null;
    }

    interface CreateUserParams {
      email: string;
      password: string;
      role: Exclude<Role, 'owner'>;
    }

    interface UpdateUserParams {
      role?: Exclude<Role, 'owner'>;
      disabled?: boolean;
      password?: string;
    }

    interface Stats {
      total_users: number;
      active_users: number;
      disabled_users: number;
    }
  }
}
