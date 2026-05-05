# Vendor matrix

OpenFiles uses Apache OpenDAL where possible. S3-compatible vendors share the S3 adapter.

| Vendor | Adapter | Version/generation support | Notes |
|---|---|---:|---|
| AWS S3 | OpenDAL S3 | ETag; versioning recommended | Best match to S3 Files behavior when bucket versioning is enabled. |
| GCP Cloud Storage | OpenDAL GCS | generation/metageneration exposed by service, mapped when available | Use service-account JSON or ADC. |
| Azure Blob | OpenDAL Azblob | ETag | Use account key or SAS. |
| Vercel Blob | OpenDAL Vercel Blob | service dependent | Token auth; metadata sidecars recommended. |
| Storj | OpenDAL S3 | ETag/version behavior endpoint-dependent | Use Gateway-MT S3 endpoint. |
| MinIO | OpenDAL S3 | ETag; versioning optional | Good local dev and edge deployment target. |
| NetApp StorageGRID | OpenDAL S3 | ETag; versioning optional | Use StorageGRID S3 endpoint. |

## Required operations

A backend should support:

- read object
- ranged read object
- write object
- delete object
- list prefix
- head/stat object
- copy object

If `copy` is unavailable, OpenFiles can fall back to read+write for rename, at higher cost.
