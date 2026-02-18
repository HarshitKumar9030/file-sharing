# Docker & Nginx Deployment Guide

## 1. Quick Start (Standalone)

Run these commands in the project directory:

```bash
docker-compose up -d --build
```
This builds your Rust app and starts it alongside a dedicated Nginx container.

---

## 2. Integration with Existing Nginx

If you already have an nginx container running on a shared network (e.g. `web_network`), modify `docker-compose.yml`:

```yaml
version: '3.8'

services:
  file-sharing-app:
    build: .
    restart: unless-stopped
    ports:
      - "8080:8080"
    volumes:
      - ./uploads:/app/uploads
    networks:
      - web_network  # Connect to existing network

networks:
  web_network:
    external: true
```

### Add this to your existing Nginx config (`/etc/nginx/conf.d/default.conf` or similar):

```nginx
server {
    listen 80;
    server_name files.yourdomain.com; # Change this

    # INCREASE THIS FOR UPLOADS
    client_max_body_size 10G; 

    location / {
        proxy_pass http://file-sharing-app:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

---

## 3. Useful Nginx Commands

**Check configuration for syntax errors:**
```bash
docker exec -it <nginx_container_name> nginx -t
```
*(Example: `docker exec -it my_nginx nginx -t`)*

**Reload Nginx without downtime (after editing config):**
```bash
docker exec -it <nginx_container_name> nginx -s reload
```

**Restart Nginx completely:**
```bash
docker restart <nginx_container_name>
```

**View Nginx logs:**
```bash
docker logs -f <nginx_container_name>
```
