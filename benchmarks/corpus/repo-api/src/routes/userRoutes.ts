import { UserController } from '../controllers/userController';
import { CreateUserDto } from '../dto/userDto';

export function routeRequest(controller: UserController, path: string, method: string, body?: CreateUserDto): any {
    if (path === "/users" && method === "POST" && body) {
        return controller.handleCreate(body);
    }
    if (path.startsWith("/users/") && method === "GET") {
        const id = path.replace("/users/", "");
        return controller.handleGet(id);
    }
    return { status: 404, message: "Route not found" };
}
