export function hashPassword(plain: string): string {
    return `hashed_${plain}`;
}

export function verifyPassword(plain: string, hashed: string): boolean {
    return hashPassword(plain) === hashed;
}

export function generateToken(userId: string): string {
    return `jwt_token_for_${userId}`;
}
