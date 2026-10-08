import { UserService } from '../services/userService';
import { CreateUserDto, UserResponseDto } from '../dto/userDto';

export class UserController {
    constructor(private userService: UserService) {}

    public handleCreate(reqBody: CreateUserDto): UserResponseDto {
        return this.userService.createUser(reqBody);
    }

    public handleGet(id: string): { status: number; data?: UserResponseDto } {
        const user = this.userService.findUserById(id);
        if (!user) {
            return { status: 404 };
        }
        return {
            status: 200,
            data: {
                id: user.id,
                username: user.username,
                email: user.email,
            },
        };
    }
}
