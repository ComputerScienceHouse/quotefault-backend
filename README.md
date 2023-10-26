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

### GET /api/reports

Returns a list of quotes which

## Database Schema

### Quotes Table

```SQL
CREATE TABLE Quotes (
    id INT4 PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    submitter VARCHAR(32) NOT NULL,
    timestamp TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    hidden BOOL NOT NULL DEFAULT FALSE,
    reported BOOL NOT NULL DEFAULT FALSE
);
```

### Quote Shards Table

```SQL
CREATE TABLE Shards (
    quote_id INT4 REFERENCES quotes(id) NOT NULL,
    index SMALLINT NOT NULL,
    body TEXT NOT NULL,
    speaker VARCHAR(32) NOT NULL,
    PRIMARY KEY (quote_id, index)
);
```
