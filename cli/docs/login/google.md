# Login with Google

The CLI signs in directly with a Google account using the authorization code flow and retrieves the user's email and profile through OpenID Connect. It requests identity scopes only; it does not access Gmail messages.

```mermaid
sequenceDiagram
    autonumber
    actor User
    participant CLI as Rust CLI
    participant Browser
    participant Google as Google OAuth
    participant Profile as Google UserInfo
    participant File as Local session.json
    participant API as worker-api
    participant DB as D1

    User->>CLI: Select Google login
    CLI->>File: Read saved session
    File-->>CLI: Session or no saved session

    opt Saved method is google
        CLI->>Google: POST https://oauth2.googleapis.com/token<br/>grant_type=refresh_token, refresh_token, client credentials
        alt Refresh succeeds
            Google-->>CLI: Access token and optional replacement refresh token
            opt Replacement refresh token returned
                CLI->>File: Save method=google and refresh_token
            end
            CLI->>Profile: GET /v1/userinfo with Bearer access token
            Profile-->>CLI: User identity including email and name
        else Refresh fails
            Google-->>CLI: Error
            CLI->>File: Remove saved session
            Note over User,CLI: Continue with interactive sign-in
        end
    end

    opt No Google session could be resumed
        CLI->>CLI: Listen on 127.0.0.1 with an available port<br/>Generate random OAuth state
        CLI->>Browser: Open Google authorization URL<br/>client_id, redirect_uri, scopes and state
        Browser->>Google: GET https://accounts.google.com/o/oauth2/auth
        Google-->>Browser: Sign-in and consent screen
        User->>Browser: Sign in and approve
        Google-->>Browser: Redirect to http://127.0.0.1:port with code and state
        Browser->>CLI: Local OAuth callback
        CLI->>CLI: Check provider errors
        CLI-->>Browser: Close-window message
        CLI->>CLI: Check matching state and authorization code
        CLI->>Google: POST https://oauth2.googleapis.com/token<br/>grant_type=authorization_code, code, redirect_uri, client credentials
        Google-->>CLI: Access token and optional refresh token
        opt Refresh token returned
            CLI->>File: Save method=google and refresh_token
        end
        CLI->>Profile: GET https://openidconnect.googleapis.com/v1/userinfo<br/>Authorization: Bearer access token
        Profile-->>CLI: User identity including email and name
    end

    CLI->>API: GET /app/users?email=...&onboarding=true<br/>Cloudflare Access credentials and X-API-Key
    API->>DB: Find application user by email
    alt Existing application user
        DB-->>API: User record
        API-->>CLI: 200 with user record
        CLI-->>User: Open application menu
    else No application user
        DB-->>API: No match
        API-->>CLI: 404
        CLI->>CLI: Call init_app_user with Google identity
        Note over User,CLI: First-time setup is currently a local placeholder
    end
```