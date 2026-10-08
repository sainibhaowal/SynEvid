export interface CreateUserDto {
    username: string;
    email: string;
    passwordHash: string;
}

export interface UpdateUserDto {
    email?: string;
    role?: string;
}

export interface UserResponseDto {
    id: string;
    username: string;
    email: string;
}
