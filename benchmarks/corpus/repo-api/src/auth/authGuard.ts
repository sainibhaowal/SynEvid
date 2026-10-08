import { verifyPassword, generateToken } from '../utils/crypto';
import { UserService } from '../services/userService';

export function authenticateUser(userService: UserService, username: string, plain: string): string | null {
    const user = userService.findUserById(username);
    if (user && verifyPassword(plain, "hashed_pw")) {
        return generateToken(user.id);
    }
    return null;
}

export function authorizeRole(requiredRole: string, actualRole: string): boolean {
    if (actualRole === "admin") {
        return true;
    }
    return actualRole === requiredRole;
}
