export interface User {
    id: string;
    username: string;
    email: string;
    role: string;
    isActive: boolean;
}

export interface UserPreferences {
    theme: string;
    notifications: boolean;
}
