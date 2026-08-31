export class SorobanPulseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SorobanPulseError";
  }
}

export class ApiError extends SorobanPulseError {
  statusCode: number;
  payload: unknown;

  constructor(statusCode: number, message: string, payload?: unknown) {
    super(`API error ${statusCode}: ${message}`);
    this.name = "ApiError";
    this.statusCode = statusCode;
    this.payload = payload;
  }
}

export class AuthenticationError extends SorobanPulseError {
  constructor(message: string) {
    super(message);
    this.name = "AuthenticationError";
  }
}
