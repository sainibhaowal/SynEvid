import { User } from '../models/userModel';
import { CreateUserDto, UserResponseDto } from '../dto/userDto';
import { hashPassword } from '../utils/crypto';

export class UserService {
    private users: User[] = [];

    public createUser(dto: CreateUserDto): UserResponseDto {
        const passwordHash = hashPassword(dto.passwordHash);
        const newUser: User = {
            id: `usr_${this.users.length + 1}`,
            username: dto.username,
            email: dto.email,
            role: "member",
            isActive: true,
        };
        this.users.push(newUser);
        return {
            id: newUser.id,
            username: newUser.username,
            email: newUser.email,
        };
    }

    public findUserById(id: string): User | undefined {
        return this.users.find(u => u.id === id);
    }

    public deleteUser(id: string): boolean {
        const idx = this.users.findIndex(u => u.id === id);
        if (idx >= 0) {
            this.users.splice(idx, 1);
            return true;
        }
        return false;
    }
}
