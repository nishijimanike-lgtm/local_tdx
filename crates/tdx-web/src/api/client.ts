const BASE = ''

async function request<T>(method: string, url: string, body?: unknown): Promise<T> {
  const opts: RequestInit = {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  }
  const res = await fetch(`${BASE}${url}`, opts)
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || `HTTP ${res.status}`)
  }
  return res.json()
}

export const api = {
  get<T>(url: string): Promise<T> { return request('GET', url) },
  post<T>(url: string, body?: unknown): Promise<T> { return request('POST', url, body) },
  put<T>(url: string, body: unknown): Promise<T> { return request('PUT', url, body) },
  patch<T>(url: string, body?: unknown): Promise<T> { return request('PATCH', url, body) },
  delete<T>(url: string): Promise<T> { return request('DELETE', url) },
}
