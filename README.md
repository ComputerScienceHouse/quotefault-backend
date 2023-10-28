# Quotefault Backend

## API

### POST /api/quote

Creates a quote

#### Post Data

```json
{
    "shards": [
        {
            "body": "Erm... what the spruce?",
            "speaker": "mcdade"
        }
    ]
}
```

### GET /api/quotes

Queries a list of quotes. With no parameters it returns the most recent 10 quotes.

#### Params

* `q={query}` - Searches the quotes for a list of space separates keywords
* `lt={qid}` - Filters for all quotes less than a given quote id. Used in pagination.
* `limit={num}` - The maximum number of entries to return (default: 10)
* `submitter={username}` - Filters for quotes submitted by a certain user
* `speaker={username}` - Filters for quotes said by a certain user

#### Response
```json
[
    {
        "submitter": {
            "cn": "Cole Stowell",
            "uid": "cole"
        },
        "timestamp": "2023-10-24T22:03:08.254364",
        "shards": [
            {
                "body": "Erm... what the spruce?",
                "speaker": {
                    "cn": "Wilson McDade",
                    "uid": "mcdade"
                }
            }
        ],
        "id": 26
    }
]
```

### GET /api/quote/{qid}

Queries for a specific quote by id.

#### Response

```json
{
    "submitter": {
        "cn": "Cole Stowell",
        "uid": "cole"
    },
    "timestamp": "2023-10-24T22:03:08.254364",
    "shards": [
        {
            "body": "Erm... what the spruce?",
            "speaker": {
                "cn": "Wilson McDade",
                "uid": "mcdade"
            }
        }
    ],
    "id": 26
}
```

### DELETE /api/quote/{qid}

Deletes a quote by id. Must be the submitter in order to delete.

### PUT /api/quote/{qid}/hide

Hides a quote by id

### POST /api/quote/{qid}/report

Reports a quote

#### Post Data

```json
{
    "reason": "Post makes fun of eboard",
}
```

### GET /api/reports

Returns a list of reports

#### Response

```json
[
    {
        "quote_id": 9,
        "reports": [
            {
                "reason": "Insults eboard",
                "timestamp": "2023-10-27T21:09:01.338863",
                "id": 10
            }
        ]
    }
]
```

### PUT /api/report/{rid}/resolve

Resolves a report

### GET /api/users

Gets a list of users

### GET /api/hidden

Gets a list of hidden quotes. Admin exclusive.

Takes and returns the same data as `/api/quotes`

#### Response

```json
[
    {
        "cn": "Cole Stowell",
        "uid": "cole"
    },
    {
        "cn": "Wilson McDade",
        "uid": "mcdade"
    }
]
```

## Database Schema

### Quotes Table

```SQL
CREATE TABLE Quotes (
    id INT4 PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    submitter VARCHAR(32) NOT NULL,
    timestamp TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    hidden BOOL NOT NULL DEFAULT FALSE
);
```

### Quote Shards Table

```SQL
CREATE TABLE Shards (
    quote_id INT4 REFERENCES quotes(id) ON DELETE CASCADE NOT NULL,
    index SMALLINT NOT NULL,
    body TEXT NOT NULL,
    speaker VARCHAR(32) NOT NULL,
    PRIMARY KEY (quote_id, index)
);
```

### Reports Table

```SQL
CREATE TABLE Reports (
    id INT4 GENERATED ALWAYS AS IDENTITY,
    quote_id INT4 REFERENCES quotes(id) ON DELETE CASCADE NOT NULL,
    reason TEXT NOT NULL,
    submitter_hash BYTEA NOT NULL,
    timestamp TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolver VARCHAR(32),
    PRIMARY KEY (quote_id, submitter_hash)
);
```
