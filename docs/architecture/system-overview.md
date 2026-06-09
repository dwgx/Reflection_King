# System Overview

Reflection King is planned as:

- Public/API service.
- Source resolver and fetch boundary.
- Transcode workers.
- Queue.
- Metadata database.
- Object storage.
- Public media delivery origin/CDN.
- Admin and audit tools.

The MVP keeps API, queue, and worker in one process so the first feature can be validated quickly.
