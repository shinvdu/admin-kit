declare namespace Api {
  /**
   * namespace Auth
   *
   * cookie session based backend api module: "auth"
   */
  namespace Auth {
    type Role = 'owner' | 'admin' | 'user';

    interface User {
      id: string;
      email: string;
      role: Role;
      disabled?: boolean;
    }

    interface LoginParams {
      email: string;
      password: string;
    }

    interface LoginResponse {
      user: User;
    }

    interface MeResponse {
      user: User;
    }

    interface ChangePasswordParams {
      current_password: string;
      new_password: string;
    }

    /** Shape expected by Soybean Admin's user menu and route guards. */
    interface UserInfo {
      userId: string;
      userName: string;
      roles: string[];
      buttons: string[];
    }
  }
}
