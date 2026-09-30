import {AppUser} from "../../worker-api/types/users";

export function preconnect() {
    const link = document.createElement("link");
    link.rel = "preconnect";
    link.href = new URL(import.meta.env.VITE_WORKER_URL).origin;
    document.head.appendChild(link);
    console.info("Connected to worker");
}

export async function whoami(): Promise<{ userId: string, origin: "jwt" | "userId" }> {
    const url = `${import.meta.env.VITE_WORKER_URL}/whoami`;
    const response = await fetch(url, {
        method: "GET",
        headers: { "Content-Type": "application/json", "X-API-Key": import.meta.env.VITE_API_KEY }
    });
    if (!response.ok)
        throw new Error(`Failed to fetch whoami: ${response.status}`);
    return await response.json();
}

export async function loadUserProfile(userId: string): Promise<AppUser> {
    const url = `${import.meta.env.VITE_WORKER_URL}/app/users?email=${userId}`;
    const response = await fetch(url, {
        method: "GET",
        headers: { "Content-Type": "application/json", "X-API-Key": import.meta.env.VITE_API_KEY }
    });
    if (!response.ok)
        throw new Error(`Failed to fetch user profile: ${response.status}`);
    const users: AppUser[] = await response.json();
    return users[0];
}