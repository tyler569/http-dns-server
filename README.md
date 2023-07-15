# async DNS server

This program implements a simple DNS server that can only serve A
records, and it implements a web server where records can be added.

By default, this listens for HTTP on `127.0.0.1:1080` and for DNS on
`127.0.0.1:1053`

## Adding records

To add a record, send an HTTP POST to the HTTP server with a JSON body
of the format

```json
{
    "name": "full.domain.name",
    "address": "1.2.3.4"
}
```
