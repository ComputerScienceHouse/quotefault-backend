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
* `involved={username}` - Filters for submitter OR speaker
* `hidden={bool}` - Filters for quotes that are hidden and visible to user (if admin, this means all hidden quotes. If normal user, this means their hidden quotes)

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
        "id": 26,
        "vote": "upvote",
        "score": 1
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
    "id": 26,
    "vote": "upvote",
    "score": 1
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

### PUT /api/quote/{qid}/resolve

Resolves all reports for a given quote with some action.

#### Params

* `hide` - Whether to hide a quote or not (Default: `false`)

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

### GET /api/version

#### Response

```json
{
    "revision": "51b5766ef81e619b0c1c46d9ab1edaa182d682f4",
    "date": "2023-10-30T17:58:49.000000000-04:00",
    "build_date": "2023-10-30T22:00:27.922648431Z"
}
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

### Votes Table

```SQL
CREATE TYPE vote AS ENUM ('upvote', 'downvote');
```

```SQL
CREATE TABLE Votes (
    quote_id INT4 REFERENCES quotes(id) ON DELETE CASCADE NOT NULL,
    vote VOTE NOT NULL,
    submitter VARCHAR(32) NOT NULL,
    timestamp TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (quote_id, submitter)
);
```
